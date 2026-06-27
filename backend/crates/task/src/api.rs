//! `/v1/tasks` 处理器：submit（入库 + 入队）/ get（org 行隔离读）/ cancel。
//!
//! 片A：worker 未接，submit 仅持久化 + LPUSH 队列；产物/计费在片B/C。
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{artifacts, groups, models, tasks};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
pub struct SubmitReq {
    /// 任务类型：video.generation / image.generation / audio.speech …
    r#type: String,
    /// 模型 slug（须 invocation=async_task）
    model: String,
    /// 标准输入字段（prompt 等）
    #[serde(default)]
    input: Value,
    /// 厂商独有参数透传
    extra: Option<Value>,
    /// 完成回调
    webhook: Option<String>,
}

#[derive(Serialize)]
pub struct SubmitResp {
    id: i64,
    #[serde(rename = "type")]
    task_type: String,
    model: String,
    status: tasks::TaskStatus,
    created_at: String,
}

#[derive(Serialize)]
pub struct ArtifactResp {
    content_type: String,
    size_bytes: Option<i64>,
    meta: Option<Value>,
    /// 临时下载 URL（presigned；对象存储未配置或签名失败时为 None）
    url: Option<String>,
}

#[derive(Serialize)]
pub struct TaskResp {
    #[serde(flatten)]
    task: tasks::Model,
    artifacts: Vec<ArtifactResp>,
}

/// 鉴权：Bearer 密钥 → KeyContext。
async fn auth(state: &AppState, headers: &HeaderMap) -> AppResult<rise_identity::KeyContext> {
    let raw = rise_identity::bearer_token(headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await
}

/// `POST /v1/tasks` —— 提交异步任务：校验模型 → 入库（Queued）→ LPUSH 队列 → 202。
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SubmitReq>,
) -> AppResult<(StatusCode, Json<SubmitResp>)> {
    let ctx = auth(&state, &headers).await?;
    let db = state.db()?;

    // 归一化：trim 后既用于校验也用于落库，避免「校验用 trim、落库用原值」导致脏数据。
    let task_type = req.r#type.trim().to_string();
    if task_type.is_empty() {
        return Err(AppError::BadRequest("type is required".into()));
    }
    // 与迁移 tasks.type varchar(48) 对齐：按字符数（非字节）校验，超长直接 400 而非 DB 500。
    if task_type.chars().count() > 48 {
        return Err(AppError::BadRequest(
            "type is too long (max 48 characters)".into(),
        ));
    }
    let model = models::Entity::find()
        .filter(models::Column::Slug.eq(&req.model))
        .one(db)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("unknown model: {}", req.model)))?;
    if model.status != models::ModelStatus::Listed {
        return Err(AppError::BadRequest("model is not listed".into()));
    }
    if model.invocation != "async_task" {
        return Err(AppError::BadRequest(
            "model is not an async-task model; use /v1/chat/completions".into(),
        ));
    }
    // 密钥模型白名单（与 chat 同策略）
    if let Some(Value::Array(list)) = &ctx.allowed_models {
        if !list.iter().any(|v| v.as_str() == Some(req.model.as_str())) {
            return Err(AppError::Forbidden);
        }
    }
    // 资金预检：余额/授信耗尽则拒绝提交（任务执行有上游真实成本，不走 chat 的后扣透支）。
    rise_billing::ensure_funds(db, ctx.org_id).await?;
    // 计费分组快照（org 当下分组 slug；无分组 = 默认价）
    let group_slug = match ctx.group_id {
        Some(gid) => groups::Entity::find_by_id(gid)
            .one(db)
            .await?
            .map(|g| g.slug),
        None => None,
    };

    let active = tasks::ActiveModel {
        org_id: Set(ctx.org_id),
        api_key_id: Set(ctx.api_key_id),
        user_id: Set(ctx.user_id),
        task_type: Set(task_type),
        model_id: Set(model.id),
        model_slug: Set(req.model.clone()),
        group_slug: Set(group_slug),
        status: Set(tasks::TaskStatus::Queued),
        input: Set(req.input),
        extra: Set(req.extra),
        // 空串/纯空格 webhook 归一为 None，避免片C worker 解析非法 URL 报错
        webhook_url: Set(req.webhook.filter(|w| !w.trim().is_empty())),
        poll_count: Set(0),
        ..Default::default()
    };
    let task = active.insert(db).await?;

    enqueue(&state, task.id).await;

    Ok((
        StatusCode::ACCEPTED,
        Json(SubmitResp {
            id: task.id,
            task_type: task.task_type,
            model: task.model_slug,
            status: task.status,
            created_at: task.created_at.to_rfc3339(),
        }),
    ))
}

/// `GET /v1/tasks/{id}` —— org 行隔离读任务 + 产物。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<Json<TaskResp>> {
    let ctx = auth(&state, &headers).await?;
    let db = state.db()?;

    // org 行隔离在查询层强制（越域当不存在，且不把他人大 JSON 行载入内存）
    let task = tasks::Entity::find()
        .filter(tasks::Column::Id.eq(id))
        .filter(tasks::Column::OrgId.eq(ctx.org_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    let rows = artifacts::Entity::find()
        .filter(artifacts::Column::TaskId.eq(id))
        .all(db)
        .await?;
    let mut arts = Vec::with_capacity(rows.len());
    for a in rows {
        // presigned 临时下载 URL（对象存储未配置/签名失败 → None，不阻断响应）
        let url = state.presign_get(&a.s3_key).await.ok();
        arts.push(ArtifactResp {
            content_type: a.content_type,
            size_bytes: a.size_bytes,
            meta: a.meta,
            url,
        });
    }

    Ok(Json(TaskResp {
        task,
        artifacts: arts,
    }))
}

/// `POST /v1/tasks/{id}/cancel` —— 尽力取消（Queued/Running → Cancelled；幂等）。
///
/// **原子条件更新**防竞态：只在状态仍为 Queued/Running 时置 Cancelled，避免 read-then-update
/// 期间 worker 把任务推进到 Succeeded/Failed 后被本次覆盖。org 隔离一并下推到 WHERE。
/// 取消一个已提交上游的 Running 任务时，后台尽力调上游 cancel（凭 vendor_task_id），闭合泄漏。
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<Json<tasks::Model>> {
    use sea_orm::sea_query::Expr;
    let ctx = auth(&state, &headers).await?;
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();

    let res = tasks::Entity::update_many()
        .filter(tasks::Column::Id.eq(id))
        .filter(tasks::Column::OrgId.eq(ctx.org_id))
        .filter(
            tasks::Column::Status.is_in([tasks::TaskStatus::Queued, tasks::TaskStatus::Running]),
        )
        .col_expr(
            tasks::Column::Status,
            Expr::value(tasks::TaskStatus::Cancelled),
        )
        .col_expr(tasks::Column::FinishedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;

    // 回读权威最新状态（越域/不存在 → 404；终态任务保持原状，幂等）。
    let latest = tasks::Entity::find()
        .filter(tasks::Column::Id.eq(id))
        .filter(tasks::Column::OrgId.eq(ctx.org_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    // 仅本次真正发生了取消转换（rows_affected==1）且任务已提交上游（有 vendor_task_id）时，
    // 才后台取消上游——避免重复 cancel 调用重复外呼。
    if res.rows_affected == 1 && latest.vendor_task_id.is_some() {
        crate::worker::spawn_upstream_cancel(state.clone(), latest.clone());
    }
    Ok(Json(latest))
}

/// 入队（尽力）：失败仅告警——任务已落库 Queued，片C 启动恢复 sweep 会补入队。
async fn enqueue(state: &AppState, id: i64) {
    let Ok(pool) = state.redis() else {
        tracing::warn!(task_id = id, "redis not configured; task queued in DB only");
        return;
    };
    match pool.get().await {
        Ok(mut conn) => {
            if let Err(e) = deadpool_redis::redis::cmd("LPUSH")
                .arg(crate::QUEUE_KEY)
                .arg(id)
                .query_async::<()>(&mut conn)
                .await
            {
                tracing::warn!(task_id = id, error = %e, "enqueue failed; will be recovered by sweep");
            }
        }
        Err(e) => {
            tracing::warn!(task_id = id, error = %e, "redis pool get failed; task queued in DB only")
        }
    }
}
