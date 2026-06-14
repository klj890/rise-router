//! 模型能力目录（定价五要素·纯能力，无价格）管理 CRUD。
//!
//! 模型实体无密钥、已派生 serde，故响应直接返回 [`models::Model`]。
//! `slug` 唯一（建表 UK）：创建/改名前先查重，命中返回 400（避免 DB 唯一约束 500）。
//! `modality`/`invocation`/`billing_unit` 为受限词表（驱动路由与计费），用白名单防拼错。
//! `display_name_i18n` 为 NOT NULL jsonb，须是非空对象（至少一个 locale）。
//! 删除遇到被 model_channels（路由）或 prices（定价）引用时 400（FK CASCADE，防静默连带删）。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{admin_guard, AppError, AppResult, AppState};
use rise_entity::{model_channels, models, prices};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::Deserialize;
use serde_json::Value;

/// 模态：决定调用模式与计费量纲族。
const KNOWN_MODALITIES: &[&str] = &["chat", "embedding", "image", "video", "audio", "rerank"];
/// 调用模式：同步流 / 异步任务。
const KNOWN_INVOCATIONS: &[&str] = &["sync_stream", "async_task"];
/// 计费量纲：按 token / 张 / 秒 / 次。
const KNOWN_BILLING_UNITS: &[&str] = &["token", "image", "second", "call"];

fn validate_in(field: &str, value: &str, allowed: &[&str]) -> AppResult<String> {
    let v = value.trim();
    if !allowed.contains(&v) {
        return Err(AppError::BadRequest(format!(
            "unknown {field} (allowed: {})",
            allowed.join(", ")
        )));
    }
    Ok(v.to_owned())
}

fn validate_slug(slug: &str) -> AppResult<String> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Err(AppError::BadRequest("slug is required".into()));
    }
    if slug.chars().count() > 128 {
        return Err(AppError::BadRequest("slug too long (max 128)".into()));
    }
    Ok(slug.to_owned())
}

/// display_name_i18n 须是非空 JSON 对象（至少一个 locale → 显示名）。
fn validate_display_name(v: &Value) -> AppResult<()> {
    match v {
        Value::Object(m) if !m.is_empty() => Ok(()),
        _ => Err(AppError::BadRequest(
            "display_name_i18n must be a non-empty object, e.g. {\"zh-CN\":\"...\"}".into(),
        )),
    }
}

#[derive(Deserialize)]
pub struct CreateReq {
    slug: String,
    /// 本地化显示名 `{"zh-CN":"...","en-US":"..."}`
    display_name_i18n: Value,
    modality: String,
    invocation: String,
    billing_unit: String,
    capabilities: Option<Value>,
    status: Option<models::ModelStatus>,
}

/// `POST /api/gateway/models`（admin）—— 新建模型目录条目。slug 查重 → 400。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<models::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let slug = validate_slug(&req.slug)?;
    validate_display_name(&req.display_name_i18n)?;
    let modality = validate_in("modality", &req.modality, KNOWN_MODALITIES)?;
    let invocation = validate_in("invocation", &req.invocation, KNOWN_INVOCATIONS)?;
    let billing_unit = validate_in("billing_unit", &req.billing_unit, KNOWN_BILLING_UNITS)?;

    if models::find_by_slug(db, &slug).await?.is_some() {
        return Err(AppError::BadRequest(format!(
            "slug '{slug}' already exists"
        )));
    }

    let m = models::ActiveModel {
        slug: Set(slug),
        display_name_i18n: Set(req.display_name_i18n),
        modality: Set(modality),
        invocation: Set(invocation),
        billing_unit: Set(billing_unit),
        capabilities: Set(req.capabilities),
        status: Set(req.status.unwrap_or(models::ModelStatus::Listed)),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(m))
}

/// `GET /api/gateway/models`（admin）—— 列出全部模型，按 id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<models::Model>>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let rows = models::Entity::find()
        .order_by_asc(models::Column::Id)
        .all(db)
        .await?;
    Ok(Json(rows))
}

/// `GET /api/gateway/models/{id}`（admin）—— 取单个模型。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<models::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let m = models::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    slug: Option<String>,
    display_name_i18n: Option<Value>,
    modality: Option<String>,
    invocation: Option<String>,
    billing_unit: Option<String>,
    /// Some=替换；显式传 `null` 也会落库为 NULL（清空 capabilities）
    capabilities: Option<Value>,
    status: Option<models::ModelStatus>,
}

/// `PUT /api/gateway/models/{id}`（admin）—— 部分更新。改 slug 时跨行查重。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<models::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let existing = models::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: models::ActiveModel = existing.into();

    if let Some(slug) = req.slug {
        let slug = validate_slug(&slug)?;
        // 改名查重：新 slug 若已被别的模型占用 → 400。
        if let Some(other) = models::find_by_slug(db, &slug).await? {
            if other.id != id {
                return Err(AppError::BadRequest(format!(
                    "slug '{slug}' already exists"
                )));
            }
        }
        am.slug = Set(slug);
    }
    if let Some(dn) = req.display_name_i18n {
        validate_display_name(&dn)?;
        am.display_name_i18n = Set(dn);
    }
    if let Some(modality) = req.modality {
        am.modality = Set(validate_in("modality", &modality, KNOWN_MODALITIES)?);
    }
    if let Some(invocation) = req.invocation {
        am.invocation = Set(validate_in("invocation", &invocation, KNOWN_INVOCATIONS)?);
    }
    if let Some(bu) = req.billing_unit {
        am.billing_unit = Set(validate_in("billing_unit", &bu, KNOWN_BILLING_UNITS)?);
    }
    if let Some(cap) = req.capabilities {
        // 显式传 null → 落 NULL（清空）；传对象 → 落该值。
        am.capabilities = Set(if cap.is_null() { None } else { Some(cap) });
    }
    if let Some(s) = req.status {
        am.status = Set(s);
    }

    let m = am.update(db).await?;
    Ok(Json(m))
}

/// `DELETE /api/gateway/models/{id}`（admin）—— 删除模型。
/// 被 model_channels（路由）或 prices（定价）引用时 400（FK CASCADE，防静默连带删历史路由/价）。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    if models::Entity::find_by_id(id).one(db).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let route_refs = model_channels::Entity::find()
        .filter(model_channels::Column::ModelId.eq(id))
        .count(db)
        .await?;
    let price_refs = prices::Entity::find()
        .filter(prices::Column::ModelId.eq(id))
        .count(db)
        .await?;
    if route_refs > 0 || price_refs > 0 {
        return Err(AppError::BadRequest(format!(
            "model is referenced by {route_refs} route(s) and {price_refs} price(s); remove them or delist the model instead"
        )));
    }
    models::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_in_trims_and_enforces_allowlist() {
        assert_eq!(
            validate_in("modality", " chat ", KNOWN_MODALITIES).unwrap(),
            "chat"
        );
        assert!(validate_in("modality", "completion", KNOWN_MODALITIES).is_err());
        assert!(validate_in("invocation", "sync_stream", KNOWN_INVOCATIONS).is_ok());
        assert!(validate_in("billing_unit", "tokens", KNOWN_BILLING_UNITS).is_err());
    }

    #[test]
    fn slug_trims_and_bounds() {
        assert_eq!(validate_slug("  gpt-4o ").unwrap(), "gpt-4o");
        assert!(validate_slug("  ").is_err());
        assert!(validate_slug(&"x".repeat(129)).is_err());
    }

    #[test]
    fn display_name_requires_nonempty_object() {
        assert!(validate_display_name(&json!({"zh-CN": "通义"})).is_ok());
        assert!(validate_display_name(&json!({})).is_err());
        assert!(validate_display_name(&json!("通义")).is_err());
        assert!(validate_display_name(&json!(null)).is_err());
    }
}
