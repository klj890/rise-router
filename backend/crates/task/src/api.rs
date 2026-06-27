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
    // 片B：补 presigned url
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

    if req.r#type.trim().is_empty() {
        return Err(AppError::BadRequest("type is required".into()));
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
        task_type: Set(req.r#type.clone()),
        model_id: Set(model.id),
        model_slug: Set(req.model.clone()),
        group_slug: Set(group_slug),
        status: Set(tasks::TaskStatus::Queued),
        input: Set(req.input),
        extra: Set(req.extra),
        webhook_url: Set(req.webhook),
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

    let arts = artifacts::Entity::find()
        .filter(artifacts::Column::TaskId.eq(id))
        .all(db)
        .await?
        .into_iter()
        .map(|a| ArtifactResp {
            content_type: a.content_type,
            size_bytes: a.size_bytes,
            meta: a.meta,
        })
        .collect();

    Ok(Json(TaskResp {
        task,
        artifacts: arts,
    }))
}

/// `POST /v1/tasks/{id}/cancel` —— 尽力取消（Queued/Running → Cancelled；幂等）。
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<Json<tasks::Model>> {
    let ctx = auth(&state, &headers).await?;
    let db = state.db()?;

    let task = tasks::Entity::find()
        .filter(tasks::Column::Id.eq(id))
        .filter(tasks::Column::OrgId.eq(ctx.org_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    if matches!(
        task.status,
        tasks::TaskStatus::Queued | tasks::TaskStatus::Running
    ) {
        let now = chrono::Utc::now().fixed_offset();
        let mut active: tasks::ActiveModel = task.into();
        active.status = Set(tasks::TaskStatus::Cancelled);
        active.finished_at = Set(Some(now));
        active.updated_at = Set(now);
        // 注：running 任务的上游 cancel 在片C接入（凭 vendor_task_id）。
        let updated = active.update(db).await?;
        return Ok(Json(updated));
    }
    Ok(Json(task))
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
