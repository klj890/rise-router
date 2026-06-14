//! relay 转发：OpenAI 兼容入口。auth → 白名单 → 加权路由 → 重试/失败转移 → 转发上游（含流式）。
//!
//! 本切片：chat/completions 非流式 + 流式 SSE 转发与计费。`/v1/tasks` 任务类留后续。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::State,
    http::{
        header::{self, HeaderName, HeaderValue},
        HeaderMap, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::time::sleep;

use rise_core::{AppError, AppResult, AppState};
use rise_entity::channels;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{resolve_route, weighted_failover_order};

/// 转发给上游的客户端头白名单（小写）。排除 auth/host/content-* 等由本网关或 reqwest 接管的头。
const FORWARD_HEADERS: [&str; 3] = ["openai-beta", "openai-organization", "openai-project"];

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

    // 4. 流式：规范化 stream=true，并注入 stream_options.include_usage（客户端未设时）以拿到末块 usage 计费。
    let is_stream = is_stream_requested(&body);
    if is_stream {
        prepare_stream_body(&mut body);
    }

    // 5. 路由（确定序）；6. 转发用加权随机序（同优先级负载均衡）
    let candidates = resolve_route(db, &model).await?;
    let ordered = weighted_failover_order(&candidates, &mut rand::thread_rng());

    // 7. 批量取候选渠道（1 次查询，避免循环内 N+1）
    let channel_ids: Vec<i32> = ordered.iter().map(|c| c.channel_id).collect();
    let channel_map: HashMap<i32, channels::Model> = channels::Entity::find()
        .filter(channels::Column::Id.is_in(channel_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|ch| (ch.id, ch))
        .collect();

    // 8. 透传白名单客户端头（认证由本网关注入上游 key）
    let fwd = forward_headers(&headers);

    let client = http_client();
    for cand in &ordered {
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
        // 9. 发送（连接级瞬时错误带退避重试；5xx 不重试同一上游，直接转移）
        match send_with_retry(client, &url, key, &body, &fwd).await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_server_error() {
                    tracing::warn!(channel = %channel.name, %status, "upstream 5xx, failover");
                    continue;
                }
                // 流式成功：边转发边扫描 usage，流结束后结算（已开始吐字节，不再 failover）
                if is_stream && status.is_success() {
                    return Ok(stream_response(resp, state, ctx, model, cand.channel_id));
                }
                // 非流式（或流式但 4xx 错误）：缓冲返回
                let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let content_type = resp.headers().get(header::CONTENT_TYPE).cloned();
                let bytes = match resp.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(channel = %channel.name, error = %e, "read upstream body failed, failover");
                        continue;
                    }
                };
                let latency_ms = started.elapsed().as_millis().try_into().ok();
                // 仅非流式 2xx 在此结算（4xx 视为未消费用量，透传不扣费）
                if status.is_success() && !is_stream {
                    settle(&state, &ctx, &model, cand.channel_id, &bytes, latency_ms).await;
                }
                let mut out_headers = HeaderMap::new();
                out_headers.insert(
                    header::CONTENT_TYPE,
                    content_type.unwrap_or_else(|| HeaderValue::from_static("application/json")),
                );
                return Ok((code, out_headers, bytes).into_response());
            }
            Err(e) => {
                tracing::warn!(channel = %channel.name, error = %e, "upstream error, failover");
                continue;
            }
        }
    }

    tracing::warn!(org_id = ctx.org_id, %model, "all upstream channels failed");
    Err(AppError::Unavailable)
}

/// 健壮判定是否请求流式：仅 absent/null/false/0/"false" 视为非流式，其余真值/疑似真值皆为流式。
/// （宽松上游会把 {"stream":"true"}/{"stream":1} 当真返回 SSE。）
fn is_stream_requested(body: &Value) -> bool {
    match body.get("stream") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(Value::String(s)) => !s.trim().eq_ignore_ascii_case("false"),
        Some(Value::Number(n)) => n.as_i64() != Some(0),
        _ => true,
    }
}

/// 规范化流式请求：stream=true（bool）+ **强制** stream_options.include_usage=true 以拿末块 usage 计费。
/// 强制（而非尊重客户端）：客户端若显式设 include_usage=false，上游将不返 usage → 计费被绕过免单。
fn prepare_stream_body(body: &mut Value) {
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    obj.insert("stream".into(), Value::Bool(true));
    let so = obj
        .entry("stream_options")
        .or_insert_with(|| Value::Object(Default::default()));
    // 客户端可能把 stream_options 传成非对象（字符串/数组）→ as_object_mut 为 None 会漏注入而绕过计费；
    // 一律规范化为对象
    if !so.is_object() {
        *so = Value::Object(Default::default());
    }
    if let Some(so_obj) = so.as_object_mut() {
        // 强制覆盖：不接受客户端的 false（否则绕过计费）
        so_obj.insert("include_usage".into(), Value::Bool(true));
    }
}

/// 从客户端头取白名单子集转发上游。
fn forward_headers(headers: &HeaderMap) -> Vec<(HeaderName, HeaderValue)> {
    FORWARD_HEADERS
        .iter()
        .filter_map(|name| {
            let h = HeaderName::from_static(name);
            headers.get(&h).map(|v| (h, v.clone()))
        })
        .collect()
}

/// 发送上游请求；**仅连接级**瞬时错误退避重试（最多 2 次尝试）。
/// 不重试超时：POST 非幂等，读超时时上游可能已处理并计费，重试会致重复扣费/重复工具调用。
/// 5xx 也不在此重试（交由调用方转移到下一渠道，避免重试同一报错上游）。
async fn send_with_retry(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    body: &Value,
    fwd: &[(HeaderName, HeaderValue)],
) -> reqwest::Result<reqwest::Response> {
    const MAX_ATTEMPTS: u32 = 2;
    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut req = client.post(url).bearer_auth(key).json(body);
        for (k, v) in fwd {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(resp) => return Ok(resp),
            Err(e) if attempt < MAX_ATTEMPTS && e.is_connect() => {
                let backoff = Duration::from_millis(200 * 2u64.pow(attempt - 1));
                tracing::warn!(url, attempt, error = %e, "connect error, retrying");
                sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// 扫描到的、供结算用的共享状态（边流边写，guard 在流被 Drop 时读取）。
#[derive(Default)]
struct ScanShared {
    usage: Option<Value>,
    request_id: Option<String>,
}

/// 结算守卫：随流生成器一起 Drop。**无论正常结束还是客户端提前断连**（axum 会 Drop 响应体流，
/// 末尾代码不再执行），都在此触发结算，杜绝断连漏单。无扫到 usage 则跳过（无从计费）。
struct SettleGuard {
    state: AppState,
    ctx: rise_identity::KeyContext,
    model: String,
    channel_id: i32,
    shared: Arc<Mutex<ScanShared>>,
}

impl Drop for SettleGuard {
    fn drop(&mut self) {
        // Drop 中绝不能再 panic：锁中毒（持锁时曾 panic）也优雅恢复，否则双 panic → 进程 abort
        let (quantity, request_id) = {
            let mut s = self
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (s.usage.take(), s.request_id.take())
        };
        let Some(quantity) = quantity else {
            tracing::debug!(model = %self.model, "stream ended without usage, skip billing");
            return;
        };
        // Drop 是同步的；用当前运行时句柄异步结算（断连发生在请求路径内，runtime 仍在）
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::error!(model = %self.model, "no runtime in SettleGuard drop; stream unbilled");
            return;
        };
        let (state, ctx, model, channel_id) = (
            self.state.clone(),
            self.ctx.clone(),
            self.model.clone(),
            self.channel_id,
        );
        handle.spawn(async move {
            do_settle(
                &state, &ctx, &model, channel_id, quantity, request_id, None, true,
            )
            .await;
        });
    }
}

/// 构造流式响应：边转发 SSE 字节边扫描 usage，结算由 [`SettleGuard`] 在流 Drop 时统一触发
/// （move 所需上下文进 stream 以满足 'static）。
fn stream_response(
    resp: reqwest::Response,
    state: AppState,
    ctx: rise_identity::KeyContext,
    model: String,
    channel_id: i32,
) -> Response {
    let content_type = resp.headers().get(header::CONTENT_TYPE).cloned();
    let body = Body::from_stream(async_stream::stream! {
        let shared = Arc::new(Mutex::new(ScanShared::default()));
        // guard 持有上下文，正常结束或提前断连被 Drop 时都会结算
        let _guard = SettleGuard {
            state,
            ctx,
            model,
            channel_id,
            shared: shared.clone(),
        };
        let mut scanner = UsageScanner::default();
        let mut request_id: Option<String> = None;
        let stream = resp.bytes_stream();
        futures_util::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => {
                    scanner.feed(&bytes, &mut request_id);
                    // 边流边镜像到 shared，供 guard 在任意时刻 Drop 时读取
                    {
                        let mut s = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if s.request_id.is_none() {
                            s.request_id.clone_from(&request_id);
                        }
                        // 始终用最新非空 usage（上游可能在后续块更新更完整用量）
                        if scanner.usage.is_some() {
                            s.usage.clone_from(&scanner.usage);
                        }
                    }
                    yield Ok::<_, std::io::Error>(bytes);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "upstream stream error");
                    yield Err(std::io::Error::other(e));
                    break;
                }
            }
        }
    });
    let mut out = HeaderMap::new();
    out.insert(
        header::CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("text/event-stream")),
    );
    (StatusCode::OK, out, body).into_response()
}

/// 增量扫描 SSE 流，提取末块 usage 与 request id（按 '\n' 切完整行，兼容跨 chunk 边界）。
#[derive(Default)]
struct UsageScanner {
    buf: Vec<u8>,
    usage: Option<Value>,
}

impl UsageScanner {
    fn feed(&mut self, bytes: &[u8], request_id: &mut Option<String>) {
        // 防 DoS：上游若发无换行的超长流，buf 会无限增长 OOM。1MB 上限：既封顶内存，又容纳
        // 大型 tool call/复杂 JSON 的单个 SSE 块（64KB 会误清这类合法行 → 漏掉末块 usage 绕过计费）。
        const MAX_BUF: usize = 1024 * 1024;
        if self.buf.len() + bytes.len() > MAX_BUF {
            tracing::warn!("SSE line buffer exceeded {MAX_BUF}B, clearing to prevent OOM");
            self.buf.clear();
        }
        self.buf.extend_from_slice(bytes);
        // '\n' 是 ASCII，按字节切行对 UTF-8 安全
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            {
                // 切片上处理免每行分配；valid UTF-8 零拷贝（非法则跳过该行）
                let line = std::str::from_utf8(&self.buf[..pos]).unwrap_or_default();
                if let Some(data) = line.trim().strip_prefix("data:") {
                    let data = data.trim();
                    if data != "[DONE]" {
                        // 流式每 token 一块，绝大多数块无 usage：先用关键字预筛，
                        // 已有 request_id 且本行无 "usage" 时直接跳过昂贵的 JSON 解析（热路径）。
                        let has_usage = data.contains("\"usage\"");
                        if request_id.is_none() || has_usage {
                            if let Ok(v) = serde_json::from_str::<Value>(data) {
                                if request_id.is_none() {
                                    if let Some(id) = v.get("id").and_then(Value::as_str) {
                                        *request_id = Some(id.to_string());
                                    }
                                }
                                // include_usage 时 usage 出现在末块（其余块为 null）
                                if has_usage && v.get("usage").is_some_and(|u| !u.is_null()) {
                                    if let Some(q) = rise_billing::extract_token_usage(&v) {
                                        self.usage = Some(q);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            self.buf.drain(..=pos);
        }
    }
}

/// 非流式结算：解析整段 JSON body → usage → 结算。
async fn settle(
    state: &AppState,
    ctx: &rise_identity::KeyContext,
    model: &str,
    channel_id: i32,
    body: &[u8],
    latency_ms: Option<i32>,
) {
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        tracing::warn!(%model, "settle: upstream body not JSON, skip billing");
        return;
    };
    let Some(quantity) = rise_billing::extract_token_usage(&parsed) else {
        tracing::debug!(%model, "settle: no usage in response, skip billing");
        return;
    };
    let request_id = parsed.get("id").and_then(Value::as_str).map(str::to_string);
    do_settle(
        state, ctx, model, channel_id, quantity, request_id, latency_ms, false,
    )
    .await;
}

/// 结算公共逻辑：组装 ChatSettlement 调 billing。错误一律吞掉只 log（at-least-serve）。
#[allow(clippy::too_many_arguments)]
async fn do_settle(
    state: &AppState,
    ctx: &rise_identity::KeyContext,
    model: &str,
    channel_id: i32,
    quantity: Value,
    request_id: Option<String>,
    latency_ms: Option<i32>,
    is_stream: bool,
) {
    let Ok(db) = state.db() else { return };
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
        is_stream,
    };
    if let Err(e) = rise_billing::settle_chat(db, s, chrono::Utc::now().fixed_offset()).await {
        tracing::error!(%model, error = %e, "settle_chat failed; call served but unbilled");
    }
}
