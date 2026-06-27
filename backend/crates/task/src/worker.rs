//! 多模态任务运行时（M5a 片B）：worker（队列消费 + 提交上游）+ poller（扫 running 续查，可恢复）
//! + 启动恢复 sweep + 完成时落产物到对象存储 + 计费结算 + webhook 回调。
//!
//! 两阶段可恢复：submit 后置 Running 并存 vendor_task_id；poller 周期扫 Running 续 poll，
//! worker 重启后凭 vendor_task_id 继续，不丢长任务。与 cancel 的原子条件更新协同防竞态。

use std::time::Duration;

use rise_core::{AppError, AppResult, AppState};
use rise_entity::{artifacts, channels, groups, model_channels, models, tasks};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, ExprTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde_json::{json, Value};

use crate::adapter::{adapter_for, PollCtx, ProducedArtifact, SubmitCtx, TaskPoll};
use crate::{PROCESSING_KEY, QUEUE_KEY};

const WORKER_CONCURRENCY: usize = 4;
const POLL_INTERVAL_SECS: u64 = 5;
const POLL_MAX: i32 = 120; // 超过则判超时失败（≈ POLL_MAX × 间隔）
/// 单个产物下载入内存上限（OOM 防护；超大文件正解是流式转存，留待片C）。
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

/// 启动任务运行时：先顺序跑恢复 sweep，再起 N 个 worker；poller 独立起。
///
/// sweep 必须在 worker 之前跑完——否则 worker 刚 BRPOPLPUSH 到 processing（DB 仍 Queued、
/// 未及置 Running）时，并发的 sweep 会把它误判为积压重新入队 → 重复消费。
pub fn spawn_task_runtime(state: AppState) {
    {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = recovery_sweep(&state).await {
                tracing::warn!(error = %e, "task recovery sweep failed");
            }
            for i in 0..WORKER_CONCURRENCY {
                let st = state.clone();
                tokio::spawn(async move { worker_loop(st, i).await });
            }
            tracing::info!(
                workers = WORKER_CONCURRENCY,
                "task workers started after sweep"
            );
        });
    }
    let st = state.clone();
    tokio::spawn(async move { poller_loop(st).await });
    tracing::info!("task runtime starting (sweep → workers; poller live)");
}

/// 共享 HTTP 客户端（单例）。reqwest::Client 内部 Arc + 连接池，克隆开销极低；
/// 复用同一实例避免频繁新建导致连接池不复用 → 套接字耗尽。
///
/// 挂自定义 DNS resolver：在**连接期**过滤私网/环回 IP——闭合 SSRF 的 DNS 重绑定缺口
/// （pre-check 解析与实际连接是两次解析，攻击者可用短 TTL 绕过；连接期过滤无此问题）。
/// 同时保护 webhook 与产物下载两条外呼路径。
fn http_client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .dns_resolver(std::sync::Arc::new(BlockPrivateResolver))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// 连接期 DNS 过滤：解析后剔除私网/环回/链路本地等 IP，全被剔则解析失败（拒绝连接）。
struct BlockPrivateResolver;
impl reqwest::dns::Resolve for BlockPrivateResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let addrs = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let safe: Vec<std::net::SocketAddr> =
                addrs.filter(|a| !is_blocked_ip(a.ip())).collect();
            if safe.is_empty() {
                let e: Box<dyn std::error::Error + Send + Sync> =
                    "host resolves only to blocked (private/loopback) addresses".into();
                return Err(e);
            }
            let iter: reqwest::dns::Addrs = Box::new(safe.into_iter());
            Ok(iter)
        })
    }
}

// ───────────────────────── worker（提交阶段）─────────────────────────

async fn worker_loop(state: AppState, idx: usize) {
    let http = http_client();
    loop {
        match next_queued(&state).await {
            Ok(Some(id)) => {
                if let Err(e) = process_submit(&state, &http, id).await {
                    tracing::error!(task_id = id, error = %e, "process_submit failed");
                    let _ = set_failed(&state, id, &format!("submit error: {e}")).await;
                }
                let _ = lrem_processing(&state, id).await;
            }
            Ok(None) => {} // BRPOPLPUSH 超时（无任务）→ 继续阻塞
            Err(e) => {
                tracing::warn!(worker = idx, error = %e, "queue poll failed; backing off");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

/// 阻塞取下一个排队任务 id（BRPOPLPUSH queued→processing，5s 超时）。
async fn next_queued(state: &AppState) -> AppResult<Option<i64>> {
    let pool = state.redis()?;
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("redis get: {e}")))?;
    let id: Option<i64> = deadpool_redis::redis::cmd("BRPOPLPUSH")
        .arg(QUEUE_KEY)
        .arg(PROCESSING_KEY)
        .arg(5)
        .query_async(&mut conn)
        .await
        .map_err(|e| AppError::Internal(format!("brpoplpush: {e}")))?;
    Ok(id)
}

async fn lrem_processing(state: &AppState, id: i64) -> AppResult<()> {
    let pool = state.redis()?;
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("redis get: {e}")))?;
    let _: i64 = deadpool_redis::redis::cmd("LREM")
        .arg(PROCESSING_KEY)
        .arg(1)
        .arg(id)
        .query_async(&mut conn)
        .await
        .map_err(|e| AppError::Internal(format!("lrem: {e}")))?;
    Ok(())
}

/// 提交一个排队任务到上游：解析路由 → adapter.submit → 原子置 Running + vendor_task_id。
async fn process_submit(state: &AppState, http: &reqwest::Client, id: i64) -> AppResult<()> {
    let db = state.db()?;
    let Some(task) = tasks::Entity::find_by_id(id).one(db).await? else {
        return Ok(()); // 已被清理
    };
    if task.status != tasks::TaskStatus::Queued {
        return Ok(()); // 已被取消/处理
    }

    let (channel, upstream_model) = resolve_route(db, task.model_id).await?;
    let adapter = adapter_for(&channel.protocol_adapter).ok_or_else(|| {
        AppError::Internal(format!("no task adapter for {}", channel.protocol_adapter))
    })?;
    let key = channel_key(&channel);

    let ctx = SubmitCtx {
        http,
        base_url: &channel.base_url,
        key: &key,
        upstream_model: &upstream_model,
        task_type: &task.task_type,
        input: &task.input,
        extra: task.extra.as_ref(),
    };
    let vendor_task_id = adapter
        .submit(&ctx)
        .await
        .map_err(|e| AppError::Internal(format!("vendor submit: {e}")))?;

    let now = chrono::Utc::now().fixed_offset();
    // 原子 Queued→Running（防与 cancel 竞态：被取消则 0 行，丢弃 vendor 任务）。
    let res = tasks::Entity::update_many()
        .filter(tasks::Column::Id.eq(id))
        .filter(tasks::Column::Status.eq(tasks::TaskStatus::Queued))
        .col_expr(
            tasks::Column::Status,
            Expr::value(tasks::TaskStatus::Running),
        )
        .col_expr(
            tasks::Column::VendorTaskId,
            Expr::value(vendor_task_id.clone()),
        )
        .col_expr(tasks::Column::ChannelId, Expr::value(channel.id))
        .col_expr(tasks::Column::StartedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;
    if res.rows_affected == 0 {
        // 已被取消：上游任务已提交但我方不再轮询 → 可能在厂商侧继续运行并计费（资源泄漏）。
        // 主动取消上游留待片C；此处 warn 输出 vendor_task_id 供运营手动排查清理。
        tracing::warn!(
            task_id = id,
            vendor_task_id = %vendor_task_id,
            "task cancelled during upstream submission; upstream task may still run and leak"
        );
    }
    Ok(())
}

// ───────────────────────── poller（运行阶段，可恢复）─────────────────────────

async fn poller_loop(state: AppState) {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    let http = http_client();
    // 在飞轮询去重：跨 tick 同一任务仍 Running 时不重复 spawn process_poll，
    // 杜绝并发 finalize 导致的重复计费/重复落产物（单实例假设）。
    let active: Arc<Mutex<HashSet<i64>>> = Arc::new(Mutex::new(HashSet::new()));
    loop {
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        match running_tasks(&state).await {
            Ok(list) => {
                for task in list {
                    // 已有在飞协程则跳过（insert 返回 false = 已存在）
                    if !active.lock().unwrap().insert(task.id) {
                        continue;
                    }
                    let st = state.clone();
                    let cl = http.clone();
                    let active = active.clone();
                    tokio::spawn(async move {
                        let tid = task.id;
                        // RAII：无论正常结束/panic/取消都从 active 移除，避免任务假死（永不再轮询）。
                        struct Guard(Arc<Mutex<HashSet<i64>>>, i64);
                        impl Drop for Guard {
                            fn drop(&mut self) {
                                if let Ok(mut s) = self.0.lock() {
                                    s.remove(&self.1);
                                }
                            }
                        }
                        let _guard = Guard(active, tid);
                        if let Err(e) = process_poll(&st, &cl, task).await {
                            tracing::error!(task_id = tid, error = %e, "process_poll failed");
                        }
                    });
                }
            }
            Err(e) => tracing::warn!(error = %e, "load running tasks failed"),
        }
    }
}

async fn running_tasks(state: &AppState) -> AppResult<Vec<tasks::Model>> {
    let db = state.db()?;
    Ok(tasks::Entity::find()
        .filter(tasks::Column::Status.eq(tasks::TaskStatus::Running))
        .order_by_asc(tasks::Column::Id)
        .limit(100)
        .all(db)
        .await?)
}

async fn process_poll(
    state: &AppState,
    http: &reqwest::Client,
    task: tasks::Model,
) -> AppResult<()> {
    let db = state.db()?;
    let Some(vendor_task_id) = task.vendor_task_id.clone() else {
        return finalize_failed(state, &task, "missing vendor_task_id").await;
    };
    let Some(channel_id) = task.channel_id else {
        return finalize_failed(state, &task, "missing channel").await;
    };
    let Some(channel) = channels::Entity::find_by_id(channel_id).one(db).await? else {
        return finalize_failed(state, &task, "channel gone").await;
    };
    let adapter = adapter_for(&channel.protocol_adapter).ok_or_else(|| {
        AppError::Internal(format!("no task adapter for {}", channel.protocol_adapter))
    })?;
    let key = channel_key(&channel);

    let ctx = PollCtx {
        http,
        base_url: &channel.base_url,
        key: &key,
        vendor_task_id: &vendor_task_id,
        poll_count: task.poll_count,
    };
    match adapter.poll(&ctx).await {
        Ok(TaskPoll::Running) => {
            if task.poll_count + 1 >= POLL_MAX {
                return finalize_failed(state, &task, "vendor task timed out").await;
            }
            // 仅自增轮询计数（不动状态）
            tasks::Entity::update_many()
                .filter(tasks::Column::Id.eq(task.id))
                .filter(tasks::Column::Status.eq(tasks::TaskStatus::Running))
                .col_expr(
                    tasks::Column::PollCount,
                    Expr::col(tasks::Column::PollCount).add(1),
                )
                .exec(db)
                .await?;
            Ok(())
        }
        Ok(TaskPoll::Succeeded { artifacts }) => finalize_succeeded(state, &task, artifacts).await,
        Ok(TaskPoll::Failed { message }) => finalize_failed(state, &task, &message).await,
        Err(e) => {
            tracing::warn!(task_id = task.id, error = %e, "vendor poll error; will retry next tick");
            Ok(())
        }
    }
}

// ───────────────────────── 完成处理 ─────────────────────────

async fn finalize_succeeded(
    state: &AppState,
    task: &tasks::Model,
    produced: Vec<ProducedArtifact>,
) -> AppResult<()> {
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();

    // 计费量纲数量（按模型 billing_unit + 任务 input 推导）。
    let billing_unit = models::Entity::find_by_id(task.model_id)
        .one(db)
        .await?
        .map(|m| m.billing_unit)
        .unwrap_or_else(|| "call".into());
    let usage = compute_usage(&billing_unit, &task.input);

    // 先落产物 + 结算，再翻 Succeeded —— 保证「客户端看到 succeeded 时产物已就绪」。
    // 落产物到对象存储（best-effort，错误记日志不阻断）。
    let bucket = state.config.s3.bucket.clone();
    for (n, art) in produced.into_iter().enumerate() {
        if let Err(e) = store_artifact(state, task.id, n, art, &bucket).await {
            tracing::error!(task_id = task.id, n, error = %e, "store artifact failed");
        }
    }

    // 计费结算（复用 chat 通用结算；失败仅告警，不翻状态——at-least-serve）。
    let group_id = group_id_from_slug(db, task.group_slug.as_deref()).await?;
    let settlement = rise_billing::ChatSettlement {
        org_id: task.org_id,
        user_id: task.user_id,
        api_key_id: task.api_key_id,
        app_id: task.app_id,
        group_id,
        model_slug: &task.model_slug,
        channel_id: task.channel_id.unwrap_or_default(),
        quantity: usage.clone(),
        latency_ms: None,
        request_id: task.request_id.clone(),
        is_stream: false,
    };
    if let Err(e) = rise_billing::settle_chat(db, settlement, now).await {
        tracing::error!(task_id = task.id, error = %e, "task settle failed; served unbilled");
    }

    // 原子抢占 Running→Succeeded（防与 cancel 竞态）。被取消则 0 行——产物/结算已落（罕见竞态，
    // 视为「工作已完成」可接受；片C 再以事务收紧）。
    let res = tasks::Entity::update_many()
        .filter(tasks::Column::Id.eq(task.id))
        .filter(tasks::Column::Status.eq(tasks::TaskStatus::Running))
        .col_expr(
            tasks::Column::Status,
            Expr::value(tasks::TaskStatus::Succeeded),
        )
        .col_expr(tasks::Column::Usage, Expr::value(usage))
        .col_expr(tasks::Column::FinishedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;
    if res.rows_affected == 0 {
        tracing::info!(
            task_id = task.id,
            "task not running at finalize (cancelled?); artifacts/charge already applied"
        );
        return Ok(());
    }

    maybe_webhook(state, task, "succeeded", None);
    Ok(())
}

async fn finalize_failed(state: &AppState, task: &tasks::Model, message: &str) -> AppResult<()> {
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();
    let res = tasks::Entity::update_many()
        .filter(tasks::Column::Id.eq(task.id))
        .filter(tasks::Column::Status.eq(tasks::TaskStatus::Running))
        .col_expr(
            tasks::Column::Status,
            Expr::value(tasks::TaskStatus::Failed),
        )
        .col_expr(tasks::Column::Error, Expr::value(message))
        .col_expr(tasks::Column::FinishedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;
    if res.rows_affected == 1 {
        maybe_webhook(state, task, "failed", Some(message.to_string()));
    }
    Ok(())
}

// ───────────────────────── 辅助 ─────────────────────────

/// 解析任务路由：取该模型一条启用的路由线 + 其启用渠道，返回 (渠道, 上游模型名)。
async fn resolve_route(
    db: &DatabaseConnection,
    model_id: i32,
) -> AppResult<(channels::Model, String)> {
    // 按优先级遍历所有启用路由线，返回首个「渠道亦启用」的——支持故障转移到次优渠道。
    let mcs = model_channels::Entity::find()
        .filter(model_channels::Column::ModelId.eq(model_id))
        .filter(model_channels::Column::Enabled.eq(true))
        .order_by_desc(model_channels::Column::Priority)
        .all(db)
        .await?;
    for mc in mcs {
        if let Some(channel) = channels::Entity::find_by_id(mc.channel_id).one(db).await? {
            if channel.status == channels::ChannelStatus::Enabled {
                return Ok((channel, mc.upstream_model_name));
            }
        }
    }
    Err(AppError::Unavailable)
}

/// 渠道凭据 key（`credentials.key`；缺省空串，mock 适配器忽略）。
fn channel_key(channel: &channels::Model) -> String {
    channel
        .credentials
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// 计费量纲数量：按 billing_unit 从任务 input 推导（缺省取保守默认）。
fn compute_usage(billing_unit: &str, input: &Value) -> Value {
    let num = |k: &str, default: i64| input.get(k).and_then(Value::as_i64).unwrap_or(default);
    match billing_unit {
        "second" => json!({ "second": num("duration", 5) }),
        "image" => json!({ "image": num("n", 1) }),
        _ => json!({ "call": 1 }),
    }
}

/// group_slug → group_id（计费快照解析；slug 为空/找不到 → None = 默认价）。
/// **DB 错误向上抛**（不静默回退默认价）：抖动时让结算失败重试，避免漏扣。
async fn group_id_from_slug(db: &DatabaseConnection, slug: Option<&str>) -> AppResult<Option<i32>> {
    let Some(slug) = slug else {
        return Ok(None);
    };
    let group = groups::Entity::find()
        .filter(groups::Column::Slug.eq(slug))
        .one(db)
        .await?;
    Ok(group.map(|g| g.id))
}

/// 落一个产物到对象存储 + 插 artifacts 行。
async fn store_artifact(
    state: &AppState,
    task_id: i64,
    n: usize,
    art: ProducedArtifact,
    bucket: &str,
) -> AppResult<()> {
    use object_store::{path::Path, ObjectStore, PutPayload};
    let db = state.db()?;
    let store = state.store()?;

    // 幂等：poller 重试（如状态翻转前瞬时故障）不重复落产物/上传。
    let key = format!("tasks/{task_id}/{n}");
    if artifacts::Entity::find()
        .filter(artifacts::Column::TaskId.eq(task_id))
        .filter(artifacts::Column::S3Key.eq(&key))
        .one(db)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let (data, content_type, meta) = match art {
        ProducedArtifact::Bytes {
            bytes,
            content_type,
            meta,
        } => (bytes, content_type, meta),
        ProducedArtifact::Url {
            url,
            content_type,
            meta,
        } => {
            // 下载上游产物后转存（私有化：客户只经 presigned 访问我方对象存储）。
            // 用带超时的 http_client，避免挂起的上游下载耗尽 worker。
            let resp = http_client()
                .get(&url)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("download artifact: {e}")))?
                // 非 2xx（404/500…）也会 Ok：显式拦截，避免把错误页当产物存。
                .error_for_status()
                .map_err(|e| AppError::Internal(format!("download artifact status: {e}")))?;
            // OOM 防护：按 Content-Length 预拦 + 读取后复核大小上限。
            // 大文件的正解是流式 put_multipart 到对象存储（不全量入内存），留待片C。
            if let Some(len) = resp.content_length() {
                if len > MAX_ARTIFACT_BYTES {
                    return Err(AppError::Internal(format!(
                        "artifact too large: {len} bytes (max {MAX_ARTIFACT_BYTES})"
                    )));
                }
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppError::Internal(format!("read artifact: {e}")))?
                .to_vec();
            if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
                return Err(AppError::Internal(format!(
                    "artifact too large: {} bytes (max {MAX_ARTIFACT_BYTES})",
                    bytes.len()
                )));
            }
            (bytes, content_type, meta)
        }
    };

    let size = data.len() as i64;
    store
        .put(
            &Path::from(key.clone()),
            PutPayload::from(bytes::Bytes::from(data)),
        )
        .await
        .map_err(|e| AppError::Internal(format!("object put: {e}")))?;

    artifacts::ActiveModel {
        task_id: Set(task_id),
        bucket: Set(bucket.to_string()),
        s3_key: Set(key),
        content_type: Set(content_type),
        size_bytes: Set(Some(size)),
        meta: Set(Some(meta)),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}

/// 回调 webhook（非阻塞：spawn 后台投递，慢/挂起的客户端 webhook 不拖垮运行时）。
fn fire_webhook(
    state: AppState,
    task_id: i64,
    task_type: String,
    url: String,
    status: &'static str,
    error: Option<String>,
) {
    tokio::spawn(async move {
        // SSRF 防护：拒绝指向私网/环回/链路本地/云元数据的回调地址。
        if !webhook_url_allowed(&url).await {
            tracing::warn!(task_id, url = %url, "webhook blocked by ssrf guard");
            mark_webhook_state(&state, task_id, "blocked").await;
            return;
        }
        let payload = json!({
            "id": task_id,
            "type": task_type,
            "status": status,
            "error": error,
        });
        let delivered = http_client()
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        mark_webhook_state(
            &state,
            task_id,
            if delivered { "delivered" } else { "failed" },
        )
        .await;
    });
}

async fn mark_webhook_state(state: &AppState, task_id: i64, st: &'static str) {
    if let Ok(db) = state.db() {
        let _ = tasks::Entity::update_many()
            .filter(tasks::Column::Id.eq(task_id))
            .col_expr(tasks::Column::WebhookState, Expr::value(st))
            .exec(db)
            .await;
    }
}

/// SSRF 防护：仅放行 http/https，且解析后的所有 IP 均非私网/环回/链路本地/未指定。
/// 主机解析失败或无可用 IP → 拒绝（保守）。
async fn webhook_url_allowed(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str().map(str::to_owned) else {
        return false;
    };
    let port = parsed.port_or_known_default().unwrap_or(80);
    let Ok(addrs) = tokio::net::lookup_host((host.as_str(), port)).await else {
        return false;
    };
    let mut any = false;
    for a in addrs {
        any = true;
        if is_blocked_ip(a.ip()) {
            return false;
        }
    }
    any
}

/// 是否为禁止外呼的 IP（私网 / 环回 / 链路本地 169.254 / 未指定 / 广播 / ULA 等）。
fn is_blocked_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local() // 169.254/16，含云元数据 169.254.169.254
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // 链路本地 fe80::/10
                || v6.to_ipv4().is_some_and(|m| is_blocked_ip(std::net::IpAddr::V4(m)))
            // IPv4-mapped
        }
    }
}

/// 若任务配了 webhook 则后台投递（无 url 直接跳过）。
fn maybe_webhook(
    state: &AppState,
    task: &tasks::Model,
    status: &'static str,
    error: Option<String>,
) {
    if let Some(url) = task.webhook_url.clone() {
        fire_webhook(
            state.clone(),
            task.id,
            task.task_type.clone(),
            url,
            status,
            error,
        );
    }
}

/// 启动恢复 sweep：把 processing 列表中 DB 仍 Queued 的任务重新入队（worker 崩溃补偿）。
/// Running 任务由 poller 凭 vendor_task_id 自然接管，无需处理。
async fn recovery_sweep(state: &AppState) -> AppResult<()> {
    let db = state.db()?;
    let pool = state.redis()?;
    let mut conn = pool
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("redis get: {e}")))?;
    let ids: Vec<i64> = deadpool_redis::redis::cmd("LRANGE")
        .arg(PROCESSING_KEY)
        .arg(0)
        .arg(-1)
        .query_async(&mut conn)
        .await
        .map_err(|e| AppError::Internal(format!("lrange: {e}")))?;

    let mut requeued = 0;
    for id in ids {
        // 单任务查询失败不中断整个 sweep（否则后续任务永久滞留 processing）。
        let still_queued = match tasks::Entity::find_by_id(id).one(db).await {
            Ok(Some(t)) => t.status == tasks::TaskStatus::Queued,
            Ok(None) => false,
            Err(e) => {
                tracing::error!(task_id = id, error = %e, "sweep: query task failed; skip");
                continue;
            }
        };
        if still_queued {
            let _: i64 = deadpool_redis::redis::cmd("LPUSH")
                .arg(QUEUE_KEY)
                .arg(id)
                .query_async(&mut conn)
                .await
                .map_err(|e| AppError::Internal(format!("lpush: {e}")))?;
            requeued += 1;
        }
        let _: i64 = deadpool_redis::redis::cmd("LREM")
            .arg(PROCESSING_KEY)
            .arg(1)
            .arg(id)
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::Internal(format!("lrem: {e}")))?;
    }
    if requeued > 0 {
        tracing::info!(requeued, "task recovery sweep re-enqueued stuck tasks");
    }
    Ok(())
}

/// 把任务置 Failed（worker 提交阶段异常用）+ 触发 webhook（与 finalize_failed 一致，
/// 避免提交阶段失败时客户端收不到通知）。
async fn set_failed(state: &AppState, id: i64, message: &str) -> AppResult<()> {
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();
    let res = tasks::Entity::update_many()
        .filter(tasks::Column::Id.eq(id))
        .filter(
            tasks::Column::Status.is_in([tasks::TaskStatus::Queued, tasks::TaskStatus::Running]),
        )
        .col_expr(
            tasks::Column::Status,
            Expr::value(tasks::TaskStatus::Failed),
        )
        .col_expr(tasks::Column::Error, Expr::value(message))
        .col_expr(tasks::Column::FinishedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;
    if res.rows_affected == 1 {
        if let Some(task) = tasks::Entity::find_by_id(id).one(db).await? {
            maybe_webhook(state, &task, "failed", Some(message.to_string()));
        }
    }
    Ok(())
}
