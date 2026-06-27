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

/// 启动任务运行时：恢复 sweep（一次）+ N 个 worker + 1 个 poller。
pub fn spawn_task_runtime(state: AppState) {
    {
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = recovery_sweep(&st).await {
                tracing::warn!(error = %e, "task recovery sweep failed");
            }
        });
    }
    for i in 0..WORKER_CONCURRENCY {
        let st = state.clone();
        tokio::spawn(async move { worker_loop(st, i).await });
    }
    let st = state.clone();
    tokio::spawn(async move { poller_loop(st).await });
    tracing::info!(
        workers = WORKER_CONCURRENCY,
        "task runtime started (worker + poller)"
    );
}

/// 共享 HTTP 客户端（适配器外呼用）。
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_default()
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
        .col_expr(tasks::Column::VendorTaskId, Expr::value(vendor_task_id))
        .col_expr(tasks::Column::ChannelId, Expr::value(channel.id))
        .col_expr(tasks::Column::StartedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;
    if res.rows_affected == 0 {
        tracing::info!(
            task_id = id,
            "task no longer queued at submit (cancelled?); skipped"
        );
    }
    Ok(())
}

// ───────────────────────── poller（运行阶段，可恢复）─────────────────────────

async fn poller_loop(state: AppState) {
    let http = http_client();
    loop {
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        match running_tasks(&state).await {
            Ok(list) => {
                for task in list {
                    let tid = task.id;
                    if let Err(e) = process_poll(&state, &http, task).await {
                        tracing::error!(task_id = tid, error = %e, "process_poll failed");
                    }
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
    let group_id = group_id_from_slug(db, task.group_slug.as_deref()).await;
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

    fire_webhook(state, task, "succeeded", None).await;
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
        fire_webhook(state, task, "failed", Some(message)).await;
    }
    Ok(())
}

// ───────────────────────── 辅助 ─────────────────────────

/// 解析任务路由：取该模型一条启用的路由线 + 其启用渠道，返回 (渠道, 上游模型名)。
async fn resolve_route(
    db: &DatabaseConnection,
    model_id: i32,
) -> AppResult<(channels::Model, String)> {
    let mc = model_channels::Entity::find()
        .filter(model_channels::Column::ModelId.eq(model_id))
        .filter(model_channels::Column::Enabled.eq(true))
        .order_by_desc(model_channels::Column::Priority)
        .one(db)
        .await?
        .ok_or(AppError::Unavailable)?;
    let channel = channels::Entity::find_by_id(mc.channel_id)
        .one(db)
        .await?
        .filter(|c| c.status == channels::ChannelStatus::Enabled)
        .ok_or(AppError::Unavailable)?;
    Ok((channel, mc.upstream_model_name))
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

/// group_slug → group_id（计费快照解析；缺省/找不到 → None = 默认价）。
async fn group_id_from_slug(db: &DatabaseConnection, slug: Option<&str>) -> Option<i32> {
    let slug = slug?;
    groups::Entity::find()
        .filter(groups::Column::Slug.eq(slug))
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|g| g.id)
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
            let resp = reqwest::get(&url)
                .await
                .map_err(|e| AppError::Internal(format!("download artifact: {e}")))?;
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppError::Internal(format!("read artifact: {e}")))?
                .to_vec();
            (bytes, content_type, meta)
        }
    };

    let size = data.len() as i64;
    let key = format!("tasks/{task_id}/{n}");
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

/// 回调 webhook（best-effort，更新 webhook_state）。
async fn fire_webhook(state: &AppState, task: &tasks::Model, status: &str, error: Option<&str>) {
    let Some(url) = task.webhook_url.clone() else {
        return;
    };
    let payload = json!({
        "id": task.id,
        "type": task.task_type,
        "status": status,
        "error": error,
    });
    let http = http_client();
    let delivered = http
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    if let Ok(db) = state.db() {
        let _ = tasks::Entity::update_many()
            .filter(tasks::Column::Id.eq(task.id))
            .col_expr(
                tasks::Column::WebhookState,
                Expr::value(if delivered { "delivered" } else { "failed" }),
            )
            .exec(db)
            .await;
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
        let still_queued = tasks::Entity::find_by_id(id)
            .one(db)
            .await?
            .map(|t| t.status == tasks::TaskStatus::Queued)
            .unwrap_or(false);
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

/// 直接把任务置 Failed（worker 提交阶段异常用；无状态守卫，仅用于刚 load 的 Queued 任务）。
async fn set_failed(state: &AppState, id: i64, message: &str) -> AppResult<()> {
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();
    tasks::Entity::update_many()
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
    Ok(())
}
