//! relay 转发：OpenAI 兼容入口。auth → 模型白名单 → resolve_route → 失败转移 → 转发上游。
//!
//! 本切片只做 chat/completions 非流式转发。计费结算、流式 SSE、加权选取、/v1/tasks 任务类
//! 留待后续切片。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

    // 4. 暂不支持流式计费：拒绝任何真值/疑似真值的 stream（不止 JSON bool true）。
    //    宽松上游会把 {"stream":"true"} / {"stream":1} 当真返回 SSE → settle 解析失败静默免单。
    //    仅 absent/null/false/0/"false" 视为非流式，其余一律拒绝。流式计费留后续切片。
    let is_stream = match body.get("stream") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(Value::String(s)) => !s.trim().eq_ignore_ascii_case("false"),
        Some(Value::Number(n)) => n.as_i64() != Some(0),
        _ => true,
    };
    if is_stream {
        return Err(AppError::BadRequest(
            "streaming is not supported yet".into(),
        ));
    }

    // 5. 路由（按故障转移顺序）
    let candidates = resolve_route(db, &model).await?;

    // 6. 失败转移转发：先批量取候选渠道（1 次查询，避免循环内 N+1），再依次尝试
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

        let started = Instant::now();
        match client.post(&url).bearer_auth(key).json(&body).send().await {
            Ok(resp) => {
                let status = resp.status();
                // 5xx 视为该渠道故障，转下一个候选
                if status.is_server_error() {
                    tracing::warn!(channel = %channel.name, %status, "upstream 5xx, failover");
                    continue;
                }
                let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                // 先取上游 Content-Type（bytes() 会消费 resp）
                let content_type = resp.headers().get(header::CONTENT_TYPE).cloned();
                // 读 body 失败时尚未向客户端写任何东西，可安全转移到下一渠道
                let bytes = match resp.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(channel = %channel.name, error = %e, "read upstream body failed, failover");
                        continue;
                    }
                };
                let latency_ms = started.elapsed().as_millis().try_into().ok();

                // 同步后扣结算：仅 2xx 成功调用计费（4xx 视为未消费用量，透传不扣费）。
                // 结算失败不影响已成功响应（at-least-serve），仅 log 供对账补记。
                if status.is_success() {
                    settle(&state, &ctx, &model, cand.channel_id, &bytes, latency_ms).await;
                }

                // 透传上游状态 + Content-Type（非 JSON 错误响应也能让客户端正确处理）
                let mut out_headers = HeaderMap::new();
                out_headers.insert(
                    header::CONTENT_TYPE,
                    content_type
                        .unwrap_or_else(|| header::HeaderValue::from_static("application/json")),
                );
                return Ok((code, out_headers, bytes).into_response());
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

/// 同步后扣结算：解析上游 usage → 调 billing 落流水 + 扣预算。
/// 结算错误一律吞掉只 log（上游已成功，不能因计费失败而拒绝已服务的响应；留待对账补记）。
async fn settle(
    state: &AppState,
    ctx: &rise_identity::KeyContext,
    model: &str,
    channel_id: i32,
    body: &[u8],
    latency_ms: Option<i32>,
) {
    let Ok(db) = state.db() else { return };
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        tracing::warn!(%model, "settle: upstream body not JSON, skip billing");
        return;
    };
    let Some(quantity) = rise_billing::extract_token_usage(&parsed) else {
        tracing::debug!(%model, "settle: no usage in response, skip billing");
        return;
    };
    let request_id = parsed.get("id").and_then(Value::as_str).map(str::to_string);
    let s = rise_billing::ChatSettlement {
        org_id: ctx.org_id,
        user_id: ctx.user_id,
        api_key_id: ctx.api_key_id,
        // App 维度计费留后续（KeyContext 暂不携带 app_id）
        app_id: None,
        group_id: ctx.group_id,
        model_slug: model,
        channel_id,
        quantity,
        latency_ms,
        request_id,
        is_stream: false,
    };
    if let Err(e) = rise_billing::settle_chat(db, s, chrono::Utc::now().fixed_offset()).await {
        tracing::error!(%model, error = %e, "settle_chat failed; call served but unbilled");
    }
}
