//! 渠道（定价五要素·纯接入；成本/售价分离）管理 CRUD。
//!
//! 渠道实体携带 `credentials` 密钥且**故意不派生 serde**，故响应一律走专用 [`ChannelView`]
//! （绝不含 credentials，仅以 `has_credentials` 标记是否已配密钥）；创建/更新走专用 DTO，
//! 更新用 `Option` 字段做「仅 Some 才改」的部分更新，避免空值覆盖与密钥误清。
//! `protocol_adapter` 限定为**已实现的协议族**白名单（防自由文本拼错导致路由永不命中）。
//! 删除遇到被 model_channels 引用时 400（防 CASCADE 静默连带删路由）。所有端点经 admin 守卫。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{channels, model_channels};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::time::Instant;

use crate::adapter::{adapter_for, AdapterConfig, ProtocolAdapter};
use crate::relay::{http_client, send_with_retry};
use crate::KNOWN_PROTOCOL_ADAPTERS;

/// 渠道响应 DTO —— **绝不含 credentials**；以 `has_credentials` 告知前端是否已配密钥。
#[derive(Serialize)]
pub struct ChannelView {
    id: i32,
    name: String,
    protocol_adapter: String,
    base_url: String,
    adapter_config: Option<Value>,
    priority: i32,
    weight: i32,
    status: channels::ChannelStatus,
    has_credentials: bool,
    // 健康管理（只读，由 test 端点/探活维护）
    response_time: Option<i32>,
    test_time: Option<DateTimeWithTimeZone>,
    test_model: Option<String>,
    auto_ban: bool,
    disabled_reason: Option<String>,
}

impl From<channels::Model> for ChannelView {
    fn from(m: channels::Model) -> Self {
        let has_credentials = !is_blank_json(&m.credentials);
        Self {
            id: m.id,
            name: m.name,
            protocol_adapter: m.protocol_adapter,
            base_url: m.base_url,
            adapter_config: m.adapter_config,
            priority: m.priority,
            weight: m.weight,
            status: m.status,
            has_credentials,
            response_time: m.response_time,
            test_time: m.test_time,
            test_model: m.test_model,
            auto_ban: m.auto_ban,
            disabled_reason: m.disabled_reason,
        }
    }
}

/// credentials 是否「为空」（null / {} / [] / "" 均视作未配置密钥）。
fn is_blank_json(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Object(m) => m.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// 校验渠道公共字段（创建/更新共用）。trim 后写回；超长/非法 400。
/// 列为无长度上限 varchar，但仍设 app 级 sane 上限防滥用。
fn validate_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if name.chars().count() > 128 {
        return Err(AppError::BadRequest("name too long (max 128)".into()));
    }
    Ok(name.to_owned())
}

fn validate_protocol_adapter(pa: &str) -> AppResult<String> {
    let pa = pa.trim();
    if !KNOWN_PROTOCOL_ADAPTERS.contains(&pa) {
        return Err(AppError::BadRequest(format!(
            "unknown protocol_adapter (allowed: {})",
            KNOWN_PROTOCOL_ADAPTERS.join(", ")
        )));
    }
    Ok(pa.to_owned())
}

fn validate_base_url(url: &str) -> AppResult<String> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::BadRequest("base_url is required".into()));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::BadRequest(
            "base_url must start with http:// or https://".into(),
        ));
    }
    if url.chars().count() > 512 {
        return Err(AppError::BadRequest("base_url too long (max 512)".into()));
    }
    Ok(url.to_owned())
}

/// weight/priority 校验：均须 ≥ 0（负权重在加权随机里无意义）。
fn validate_weight(w: i32) -> AppResult<()> {
    if w < 0 {
        return Err(AppError::BadRequest("weight must be >= 0".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    protocol_adapter: String,
    base_url: String,
    /// 密钥配置 jsonb（如 `{"key": "sk-..."}`）；缺省 = 暂不配密钥（存 `{}`）
    credentials: Option<Value>,
    adapter_config: Option<Value>,
    priority: Option<i32>,
    weight: Option<i32>,
    status: Option<channels::ChannelStatus>,
    /// 是否允许被自动禁用；缺省走 DB 默认 true。
    auto_ban: Option<bool>,
    /// 渠道测试默认模型。
    test_model: Option<String>,
}

/// 测试模型名 trim；空白 → None（视为未设）。
fn normalize_test_model(tm: Option<String>) -> Option<String> {
    tm.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// `POST /api/gateway/channels`（admin）—— 新建渠道。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<ChannelView>> {
    rise_identity::require(&state, &headers, "gateway.manage").await?;
    let db = state.db()?;

    let name = validate_name(&req.name)?;
    let protocol_adapter = validate_protocol_adapter(&req.protocol_adapter)?;
    let base_url = validate_base_url(&req.base_url)?;
    let priority = req.priority.unwrap_or(0);
    // 默认 weight=1（非 0）：使新渠道在加权随机选择中即刻可被选中（DB 默认 0 仅为建表占位）。
    let weight = req.weight.unwrap_or(1);
    validate_weight(weight)?;

    let mut m = channels::ActiveModel {
        name: Set(name),
        protocol_adapter: Set(protocol_adapter),
        base_url: Set(base_url),
        credentials: Set(req.credentials.unwrap_or(Value::Object(Default::default()))),
        adapter_config: Set(req.adapter_config),
        priority: Set(priority),
        weight: Set(weight),
        status: Set(req.status.unwrap_or(channels::ChannelStatus::Enabled)),
        ..Default::default()
    };
    // auto_ban 缺省走 DB default(true)；test_model 空白视为未设
    if let Some(ab) = req.auto_ban {
        m.auto_ban = Set(ab);
    }
    if let Some(tm) = normalize_test_model(req.test_model) {
        m.test_model = Set(Some(tm));
    }
    let m = m.insert(db).await?;
    Ok(Json(m.into()))
}

/// `GET /api/gateway/channels`（admin）—— 列出全部渠道（脱敏），按 id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<ChannelView>>> {
    rise_identity::require(&state, &headers, "gateway.manage").await?;
    let db = state.db()?;
    let rows = channels::Entity::find()
        .order_by_asc(channels::Column::Id)
        .all(db)
        .await?;
    Ok(Json(rows.into_iter().map(ChannelView::from).collect()))
}

/// `GET /api/gateway/channels/{id}`（admin）—— 取单个渠道（脱敏）。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<ChannelView>> {
    rise_identity::require(&state, &headers, "gateway.manage").await?;
    let db = state.db()?;
    let m = channels::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m.into()))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    protocol_adapter: Option<String>,
    base_url: Option<String>,
    /// Some=替换密钥；None=保持不变（不会被空值覆盖）
    credentials: Option<Value>,
    adapter_config: Option<Value>,
    priority: Option<i32>,
    weight: Option<i32>,
    status: Option<channels::ChannelStatus>,
    auto_ban: Option<bool>,
    /// 空字符串 = 清空 test_model；非空 = 设值；None = 不变。
    test_model: Option<String>,
}

/// `PUT /api/gateway/channels/{id}`（admin）—— 部分更新：仅请求体里 Some 的字段入库。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<ChannelView>> {
    rise_identity::require(&state, &headers, "gateway.manage").await?;
    let db = state.db()?;

    let existing = channels::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: channels::ActiveModel = existing.into();

    if let Some(name) = req.name {
        am.name = Set(validate_name(&name)?);
    }
    if let Some(pa) = req.protocol_adapter {
        am.protocol_adapter = Set(validate_protocol_adapter(&pa)?);
    }
    if let Some(url) = req.base_url {
        am.base_url = Set(validate_base_url(&url)?);
    }
    if let Some(cred) = req.credentials {
        am.credentials = Set(cred);
    }
    if let Some(cfg) = req.adapter_config {
        am.adapter_config = Set(Some(cfg));
    }
    if let Some(p) = req.priority {
        am.priority = Set(p);
    }
    if let Some(w) = req.weight {
        validate_weight(w)?;
        am.weight = Set(w);
    }
    if let Some(s) = req.status {
        am.status = Set(s);
    }
    if let Some(ab) = req.auto_ban {
        am.auto_ban = Set(ab);
    }
    if let Some(tm) = req.test_model {
        let tm = tm.trim();
        am.test_model = Set((!tm.is_empty()).then(|| tm.to_string()));
    }

    let m = am.update(db).await?;
    Ok(Json(m.into()))
}

/// `DELETE /api/gateway/channels/{id}`（admin）—— 删除渠道。
/// 被 model_channels 引用时 400（FK 为 CASCADE，硬删会静默连带删路由，故先拦截让管理员清依赖或改禁用）。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    rise_identity::require(&state, &headers, "gateway.manage").await?;
    let db = state.db()?;

    if channels::Entity::find_by_id(id).one(db).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let route_refs = model_channels::Entity::find()
        .filter(model_channels::Column::ChannelId.eq(id))
        .count(db)
        .await?;
    if route_refs > 0 {
        return Err(AppError::BadRequest(format!(
            "channel is referenced by {route_refs} route(s); remove them or disable the channel instead"
        )));
    }
    channels::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
pub struct TestReq {
    /// 指定测试的上游模型；缺省走渠道 test_model / 首条路由。
    #[serde(default)]
    model: Option<String>,
}

/// 渠道连通性测试结果（前端展示）。
#[derive(Serialize)]
pub struct TestResult {
    pub(crate) ok: bool,
    /// 上游 HTTP 状态码（0 = 未发出/连接失败）。
    pub(crate) status: u16,
    pub(crate) latency_ms: i64,
    /// 实际测试的上游模型名。
    pub(crate) model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) usage: Option<Value>,
}

/// `POST /api/gateway/channels/{id}/test`（admin）—— 连通性测试。
///
/// 用渠道的协议适配器构造一个最小请求**真打上游**（不计费、不 failover），过五层判定漏斗，
/// 写回测速。复用 relay 同一套 adapter + 发送逻辑 → "测试通过 = 真实可转发"，自动覆盖模型
/// 映射 / adapter_config / 协议转换 / 错误归一。本切片做非流式探测（流式探测留后续）。
pub async fn test(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<TestReq>,
) -> AppResult<Json<TestResult>> {
    rise_identity::require(&state, &headers, "gateway.manage").await?;
    let db = state.db()?;

    let channel = channels::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let outcome = test_channel_once(db, &channel, normalize_test_model(req.model)).await?;

    // 测速写回（同步即可：本就慢请求；只更新两列，其余字段不动）
    if let Err(e) = write_back_test(db, id, outcome.latency_ms).await {
        tracing::warn!(channel = %channel.name, error = %e, "write back channel test time failed");
    }
    Ok(Json(outcome))
}

/// 测一个渠道一次（发最小请求真打上游 → 五层判定 → 测速）。不写库、不改状态。
/// test 端点与定时探活共用，保证"测试通过 = 真实可转发"。
pub(crate) async fn test_channel_once(
    db: &sea_orm::DatabaseConnection,
    channel: &channels::Model,
    model_override: Option<String>,
) -> AppResult<TestResult> {
    let adapter = adapter_for(&channel.protocol_adapter)
        .ok_or_else(|| AppError::BadRequest("channel has unknown protocol_adapter".into()))?;
    let upstream_model = select_test_model(db, channel, model_override).await?;

    let cfg = AdapterConfig::new(channel.adapter_config.as_ref());
    let key = channel
        .credentials
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let probe = serde_json::json!({
        "model": upstream_model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 16,
    });
    let url = adapter.request_url(&channel.base_url, &upstream_model, false, &cfg);
    let body = adapter.build_request_body(&probe, &upstream_model, &cfg);
    let auth = adapter.auth_headers(key, &cfg);

    let started = Instant::now();
    let send_result = send_with_retry(http_client(), &url, &auth, &body, &[]).await;
    let latency_ms = started.elapsed().as_millis() as i64;

    let mut out = TestResult {
        ok: false,
        status: 0,
        latency_ms,
        model: upstream_model,
        error: None,
        usage: None,
    };
    match send_result {
        Err(e) => out.error = Some(format!("request failed: {e}")),
        Ok(resp) => {
            let status = resp.status();
            out.status = status.as_u16();
            let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let bytes = resp.bytes().await.unwrap_or_default();
            if !status.is_success() {
                out.error = Some(extract_error_message(adapter.as_ref(), code, &bytes, &cfg));
            } else {
                match validate_probe(adapter.as_ref(), &bytes, &cfg) {
                    Ok(usage) => {
                        out.ok = true;
                        out.usage = usage;
                    }
                    Err(msg) => out.error = Some(msg),
                }
            }
        }
    }
    Ok(out)
}

/// 选测试上游模型：override > 渠道 test_model > 首条路由（都无 → 400）。
async fn select_test_model(
    db: &sea_orm::DatabaseConnection,
    channel: &channels::Model,
    model_override: Option<String>,
) -> AppResult<String> {
    if let Some(m) = model_override {
        return Ok(m);
    }
    if let Some(m) = channel.test_model.clone() {
        return Ok(m);
    }
    first_route_model(db, channel.id).await?.ok_or_else(|| {
        AppError::BadRequest("no test model: set test_model or add a route to this channel".into())
    })
}

/// 取该渠道的首条路由（按 id 升序）的上游模型名。
async fn first_route_model(
    db: &sea_orm::DatabaseConnection,
    channel_id: i32,
) -> AppResult<Option<String>> {
    Ok(model_channels::Entity::find()
        .filter(model_channels::Column::ChannelId.eq(channel_id))
        .order_by_asc(model_channels::Column::Id)
        .one(db)
        .await?
        .map(|m| m.upstream_model_name))
}

/// 写回测速结果（仅 response_time + test_time，其余字段 NotSet 不动）。test 端点与探活共用。
pub(crate) async fn write_back_test(
    db: &sea_orm::DatabaseConnection,
    id: i32,
    latency_ms: i64,
) -> AppResult<()> {
    let am = channels::ActiveModel {
        id: Set(id),
        response_time: Set(Some(latency_ms.min(i32::MAX as i64) as i32)),
        test_time: Set(Some(chrono::Utc::now().fixed_offset())),
        ..Default::default()
    };
    am.update(db).await?;
    Ok(())
}

/// 上游非 2xx：经适配器错误归一取 message（拿不到则截断原始 body）。
fn extract_error_message(
    adapter: &dyn ProtocolAdapter,
    code: StatusCode,
    bytes: &[u8],
    cfg: &AdapterConfig,
) -> String {
    adapter
        .convert_error(code, bytes, cfg)
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).chars().take(300).collect())
}

/// 五层判定漏斗（非流式）：①发送成功 ②HTTP 200（调用方已判）③可解析 JSON
/// ④body 内无 error 字段（挡"200 但 body 是错误"）⑤有内容 choices/content（挡"200 空响应"）。
/// 返回提取到的 usage（计费链路信息，缺失不致 fail）。
fn validate_probe(
    adapter: &dyn ProtocolAdapter,
    bytes: &[u8],
    cfg: &AdapterConfig,
) -> Result<Option<Value>, String> {
    let parsed: Value = serde_json::from_slice(bytes)
        .map_err(|_| "upstream 200 but body is not JSON".to_string())?;
    // 部分宽松上游会在 200 body 里藏 error
    if let Some(err) = parsed.get("error") {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error in 200 body");
        return Err(format!("upstream error in 200 body: {msg}"));
    }
    let converted = adapter.convert_response(&parsed, cfg);
    let oai = converted.as_ref().unwrap_or(&parsed);
    let has_content = oai
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .map(|c| {
            let m = c.get("message");
            let text = m
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let tools = m
                .and_then(|m| m.get("tool_calls"))
                .and_then(Value::as_array)
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            text || tools
        })
        .unwrap_or(false);
    if !has_content {
        return Err("upstream 200 but empty response (no content/choices)".into());
    }
    Ok(rise_billing::extract_token_usage(oai))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn name_trims_and_rejects_blank_or_too_long() {
        assert_eq!(validate_name("  ch-1 ").unwrap(), "ch-1");
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"x".repeat(129)).is_err());
    }

    #[test]
    fn protocol_adapter_allows_only_known_families() {
        assert_eq!(
            validate_protocol_adapter(" openai_compatible ").unwrap(),
            "openai_compatible"
        );
        // 拼错（连字符）必须被拒，否则路由永不命中。
        assert!(validate_protocol_adapter("openai-compatible").is_err());
        // 已实现的协议族放行
        assert_eq!(validate_protocol_adapter("anthropic").unwrap(), "anthropic");
        // 全新协议族（尚无适配器）必须被拒
        assert!(validate_protocol_adapter("cohere_native").is_err());
    }

    #[test]
    fn base_url_requires_http_scheme_and_bounded_len() {
        assert_eq!(
            validate_base_url(" https://api.x.com/v1 ").unwrap(),
            "https://api.x.com/v1"
        );
        assert!(validate_base_url("api.x.com").is_err());
        assert!(validate_base_url("").is_err());
        assert!(validate_base_url(&format!("https://x/{}", "a".repeat(512))).is_err());
    }

    #[test]
    fn weight_must_be_non_negative() {
        assert!(validate_weight(0).is_ok());
        assert!(validate_weight(5).is_ok());
        assert!(validate_weight(-1).is_err());
    }

    #[test]
    fn blank_json_detects_unconfigured_credentials() {
        assert!(is_blank_json(&json!(null)));
        assert!(is_blank_json(&json!({})));
        assert!(is_blank_json(&json!([])));
        assert!(is_blank_json(&json!("")));
        assert!(!is_blank_json(&json!({"key": "sk-x"})));
    }

    #[test]
    fn normalize_test_model_trims_and_blanks_to_none() {
        assert_eq!(
            normalize_test_model(Some(" gpt ".into())),
            Some("gpt".into())
        );
        assert_eq!(normalize_test_model(Some("   ".into())), None);
        assert_eq!(normalize_test_model(None), None);
    }

    fn probe(body: &[u8]) -> Result<Option<Value>, String> {
        validate_probe(
            &crate::adapter::OpenAiCompatAdapter,
            body,
            &AdapterConfig::new(None),
        )
    }

    #[test]
    fn probe_passes_on_valid_response_with_usage() {
        let r = probe(br#"{"choices":[{"message":{"role":"assistant","content":"hi"}}],"usage":{"prompt_tokens":1,"completion_tokens":2}}"#).unwrap();
        assert!(r.is_some()); // usage 提取到
    }

    #[test]
    fn probe_rejects_error_in_200_body() {
        // 坑①：200 但 body 是错误
        assert!(probe(br#"{"error":{"message":"quota exceeded"}}"#).is_err());
    }

    #[test]
    fn probe_rejects_empty_response() {
        // 坑②：200 但空响应
        assert!(probe(br#"{"choices":[{"message":{"role":"assistant","content":""}}]}"#).is_err());
        assert!(probe(br#"{"choices":[]}"#).is_err());
    }

    #[test]
    fn probe_rejects_non_json() {
        assert!(probe(b"<html>502 Bad Gateway</html>").is_err());
    }

    #[test]
    fn probe_accepts_tool_calls_without_content() {
        let r = probe(br#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"c1"}]}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#);
        assert!(r.is_ok());
    }
}
