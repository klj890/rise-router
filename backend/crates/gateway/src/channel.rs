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
use rise_core::{admin_guard, AppError, AppResult, AppState};
use rise_entity::{channels, model_channels};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}

/// `POST /api/gateway/channels`（admin）—— 新建渠道。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<ChannelView>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let name = validate_name(&req.name)?;
    let protocol_adapter = validate_protocol_adapter(&req.protocol_adapter)?;
    let base_url = validate_base_url(&req.base_url)?;
    let priority = req.priority.unwrap_or(0);
    // 默认 weight=1（非 0）：使新渠道在加权随机选择中即刻可被选中（DB 默认 0 仅为建表占位）。
    let weight = req.weight.unwrap_or(1);
    validate_weight(weight)?;

    let m = channels::ActiveModel {
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
    let m = m.insert(db).await?;
    Ok(Json(m.into()))
}

/// `GET /api/gateway/channels`（admin）—— 列出全部渠道（脱敏），按 id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<ChannelView>>> {
    admin_guard(&state, &headers)?;
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
    admin_guard(&state, &headers)?;
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
}

/// `PUT /api/gateway/channels/{id}`（admin）—— 部分更新：仅请求体里 Some 的字段入库。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<ChannelView>> {
    admin_guard(&state, &headers)?;
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
    admin_guard(&state, &headers)?;
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
        assert!(validate_protocol_adapter("anthropic").is_err());
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
}
