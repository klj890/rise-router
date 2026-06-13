//! relay 转发：OpenAI 兼容入口。auth → 模型白名单 → resolve_route → 失败转移 → 转发上游。
//!
//! 本切片只做 chat/completions 非流式转发。计费结算、流式 SSE、加权选取、/v1/tasks 任务类
//! 留待后续切片。

use std::sync::OnceLock;
use std::time::Duration;

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use std::collections::HashMap;

use rise_core::{AppError, AppResult, AppState};
use rise_entity::channels;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::Value;

use crate::resolve_route;

/// 进程级共享 HTTP 客户端（连接池复用，避免每请求重建）。
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("build reqwest client")
    })
}

/// OpenAI 兼容入口（挂在根 /v1）。
pub fn relay_routes() -> Router<AppState> {
    Router::new().route("/v1/chat/completions", post(chat_completions))
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> AppResult<Response> {
    let db = state.db()?;

    // 1. 鉴权：Bearer → KeyContext
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    // 2. 取 model
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::BadRequest("missing 'model'".into()))?
        .to_string();

    // 3. 模型白名单（密钥限定时）
    if let Some(allowed) = ctx.allowed_models.as_ref().and_then(Value::as_array) {
        if !allowed.iter().any(|m| m.as_str() == Some(model.as_str())) {
            return Err(AppError::Forbidden);
        }
    }

    // 4. 路由（按故障转移顺序）
    let candidates = resolve_route(db, &model).await?;

    // 5. 失败转移转发：先批量取候选渠道（1 次查询，避免循环内 N+1），再依次尝试
    let channel_ids: Vec<i32> = candidates.iter().map(|c| c.channel_id).collect();
    let channel_map: HashMap<i32, channels::Model> = channels::Entity::find()
        .filter(channels::Column::Id.is_in(channel_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|ch| (ch.id, ch))
        .collect();

    let client = http_client();
    for cand in &candidates {
        let Some(channel) = channel_map.get(&cand.channel_id) else {
            continue;
        };
        let key = channel
            .credentials
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // 模型映射到上游真实名
        body["model"] = Value::String(cand.upstream_model_name.clone());
        let url = format!(
            "{}/chat/completions",
            channel.base_url.trim_end_matches('/')
        );

        match client.post(&url).bearer_auth(key).json(&body).send().await {
            Ok(resp) => {
                let status = resp.status();
                // 5xx 视为该渠道故障，转下一个候选
                if status.is_server_error() {
                    tracing::warn!(channel = %channel.name, %status, "upstream 5xx, failover");
                    continue;
                }
                let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                // 透传上游状态 + body
                return Ok(
                    (code, [(header::CONTENT_TYPE, "application/json")], bytes).into_response()
                );
            }
            Err(e) => {
                tracing::warn!(channel = %channel.name, error = %e, "upstream error, failover");
                continue;
            }
        }
    }

    // 所有候选渠道均失败
    tracing::warn!(org_id = ctx.org_id, %model, "all upstream channels failed");
    Err(AppError::Unavailable)
}
