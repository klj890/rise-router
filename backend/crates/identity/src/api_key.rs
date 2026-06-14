//! 虚拟密钥（借鉴 LiteLLM virtual key）管理 CRUD。
//!
//! 密钥实体携带 `key_hash` 凭据且**故意不派生 serde**，故响应一律走 [`ApiKeyView`]（绝不含 hash）。
//! 创建时服务端生成随机明文密钥 `sk-rr-…`，仅哈希入库（[`crate::hash_key`] = sha256），
//! **明文仅在创建响应里回显一次**（与 GitHub/OpenAI 同款一次性展示，丢失只能轮换）。
//! 预算/模型白名单/过期挂在密钥上；budget_used 由计费维护，不经管理端点改。所有端点经 admin 守卫。

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rand::Rng;
use rise_core::{admin_guard, AppError, AppResult, AppState};
use rise_entity::{api_keys, organizations};
use rust_decimal::Decimal;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 密钥响应 DTO —— **绝不含 key_hash**。
#[derive(Serialize)]
pub struct ApiKeyView {
    id: i32,
    org_id: i32,
    user_id: Option<i32>,
    app_id: Option<i32>,
    name: String,
    allowed_models: Option<Value>,
    budget_limit: Option<Decimal>,
    budget_used: Decimal,
    expires_at: Option<DateTimeWithTimeZone>,
    status: api_keys::KeyStatus,
}

impl From<api_keys::Model> for ApiKeyView {
    fn from(m: api_keys::Model) -> Self {
        Self {
            id: m.id,
            org_id: m.org_id,
            user_id: m.user_id,
            app_id: m.app_id,
            name: m.name,
            allowed_models: m.allowed_models,
            budget_limit: m.budget_limit,
            budget_used: m.budget_used,
            expires_at: m.expires_at,
            status: m.status,
        }
    }
}

/// 生成随机明文密钥：`sk-rr-` + 40 位 [A-Za-z0-9]（≈238 bits 熵，碰撞可忽略）。
fn generate_raw_key() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let token: String = (0..40)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect();
    format!("sk-rr-{token}")
}

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

/// allowed_models 须是字符串数组（模型 slug 白名单）；null/缺省 = 不限。
fn validate_allowed_models(v: &Value) -> AppResult<()> {
    match v {
        Value::Array(a) if a.iter().all(Value::is_string) => Ok(()),
        _ => Err(AppError::BadRequest(
            "allowed_models must be an array of model slug strings".into(),
        )),
    }
}

/// budget_limit 若提供须 > 0（round 到 numeric(18,8)）。返回规整值。
fn clean_budget(b: Option<Decimal>) -> AppResult<Option<Decimal>> {
    match b {
        None => Ok(None),
        Some(b) => {
            let b = b.round_dp(8);
            if b <= Decimal::ZERO {
                return Err(AppError::BadRequest("budget_limit must be > 0".into()));
            }
            Ok(Some(b))
        }
    }
}

#[derive(Deserialize)]
pub struct CreateReq {
    org_id: i32,
    name: String,
    user_id: Option<i32>,
    app_id: Option<i32>,
    /// 模型 slug 白名单数组；null/缺省 = 不限
    allowed_models: Option<Value>,
    /// 预算上限（元）；缺省 = 不限
    budget_limit: Option<Decimal>,
    /// 过期时间；缺省 = 永不过期
    expires_at: Option<DateTimeWithTimeZone>,
}

#[derive(Serialize)]
pub struct CreateResp {
    /// **明文密钥，仅此一次返回**，请立即妥善保存（库内仅存哈希，丢失只能轮换）。
    key: String,
    api_key: ApiKeyView,
}

/// `POST /api/identity/api-keys`（admin）—— 生成密钥，明文仅回显一次。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<CreateResp>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    // org 必须存在（FK CASCADE；不存在则 insert FK 失败 → 500）。
    if organizations::Entity::find_by_id(req.org_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::BadRequest("org_id not found".into()));
    }
    let name = validate_name(&req.name)?;
    if let Some(ref v) = req.allowed_models {
        validate_allowed_models(v)?;
    }
    let budget_limit = clean_budget(req.budget_limit)?;

    let raw = generate_raw_key();
    let key_hash = crate::hash_key(&raw);

    let m = api_keys::ActiveModel {
        org_id: Set(req.org_id),
        user_id: Set(req.user_id),
        app_id: Set(req.app_id),
        key_hash: Set(key_hash),
        name: Set(name),
        allowed_models: Set(req.allowed_models),
        budget_limit: Set(budget_limit),
        budget_used: Set(Decimal::ZERO),
        expires_at: Set(req.expires_at),
        status: Set(api_keys::KeyStatus::Enabled),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(CreateResp {
        key: raw,
        api_key: m.into(),
    }))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// 可选按组织过滤
    org_id: Option<i32>,
    limit: Option<u64>,
    /// 游标：上页末条 id；返回 id < cursor 的更早密钥
    cursor: Option<i32>,
}

/// `GET /api/identity/api-keys`（admin）—— 列出密钥（脱敏），可按 org 过滤，id 倒序游标分页。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ApiKeyView>>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let limit = q.limit.unwrap_or(50).min(200);
    let mut query = api_keys::Entity::find();
    if let Some(oid) = q.org_id {
        query = query.filter(api_keys::Column::OrgId.eq(oid));
    }
    if let Some(cursor) = q.cursor {
        query = query.filter(api_keys::Column::Id.lt(cursor));
    }
    let rows = query
        .order_by_desc(api_keys::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    Ok(Json(rows.into_iter().map(ApiKeyView::from).collect()))
}

/// `GET /api/identity/api-keys/{id}`（admin）—— 取单个密钥（脱敏）。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<ApiKeyView>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let m = api_keys::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m.into()))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    /// Some(数组)=替换白名单；None=不变（清空回不限请轮换密钥）
    allowed_models: Option<Value>,
    /// Some=设上限（>0）；None=不变
    budget_limit: Option<Decimal>,
    /// Some=设过期；None=不变
    expires_at: Option<DateTimeWithTimeZone>,
    /// 仅允许 Enabled/Disabled；Exhausted 由计费置位，不可手动设
    status: Option<api_keys::KeyStatus>,
}

/// `PUT /api/identity/api-keys/{id}`（admin）—— 部分更新（不改 key_hash/org_id；轮换=新建）。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<ApiKeyView>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let existing = api_keys::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: api_keys::ActiveModel = existing.into();

    if let Some(name) = req.name {
        am.name = Set(validate_name(&name)?);
    }
    if let Some(v) = req.allowed_models {
        validate_allowed_models(&v)?;
        am.allowed_models = Set(Some(v));
    }
    if let Some(b) = req.budget_limit {
        am.budget_limit = Set(clean_budget(Some(b))?);
    }
    if let Some(exp) = req.expires_at {
        am.expires_at = Set(Some(exp));
    }
    if let Some(status) = req.status {
        if status == api_keys::KeyStatus::Exhausted {
            return Err(AppError::BadRequest(
                "cannot set status to exhausted manually (billing-managed)".into(),
            ));
        }
        am.status = Set(status);
    }

    let m = am.update(db).await?;
    Ok(Json(m.into()))
}

/// `DELETE /api/identity/api-keys/{id}`（admin）—— 删除密钥。
/// usage_logs 软引用 api_key_id（无 FK），删除不影响历史账；运营上更建议禁用而非删除。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let res = api_keys::Entity::delete_by_id(id).exec(db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generated_key_has_prefix_and_length() {
        let k = generate_raw_key();
        assert!(k.starts_with("sk-rr-"));
        assert_eq!(k.len(), 6 + 40);
        // 两次生成应不同（随机）
        assert_ne!(generate_raw_key(), generate_raw_key());
    }

    #[test]
    fn allowed_models_must_be_string_array() {
        assert!(validate_allowed_models(&json!(["gpt-4o", "claude"])).is_ok());
        assert!(validate_allowed_models(&json!([])).is_ok());
        assert!(validate_allowed_models(&json!([1, 2])).is_err());
        assert!(validate_allowed_models(&json!({"a": 1})).is_err());
        assert!(validate_allowed_models(&json!("gpt-4o")).is_err());
    }

    #[test]
    fn budget_must_be_positive() {
        assert_eq!(clean_budget(None).unwrap(), None);
        assert_eq!(
            clean_budget(Some("10.5".parse().unwrap())).unwrap(),
            Some("10.5".parse().unwrap())
        );
        assert!(clean_budget(Some(Decimal::ZERO)).is_err());
        assert!(clean_budget(Some("-1".parse().unwrap())).is_err());
    }
}
