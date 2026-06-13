//! 身份与组织域：organizations / api_keys / 密钥鉴权（verify_key）。
//!
//! 纯校验在 [`auth`]（无 DB，单测覆盖）；[`verify_key`] 是 DB 编排，relay 鉴权复用。
//! 用户登录/注册（users + 密码）作为后续 identity 子片。

mod auth;

pub use auth::{evaluate_key, hash_key, KeyContext, KeyError};

use axum::{
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap},
    routing::get,
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{api_keys, organizations};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

/// 校验原始密钥 → KeyContext。失败映射到合适的 HTTP 状态（401/403/429）。
pub async fn verify_key(
    db: &DatabaseConnection,
    raw_key: &str,
    now: DateTimeWithTimeZone,
) -> AppResult<KeyContext> {
    let hash = hash_key(raw_key);
    // 单次 JOIN 取密钥 + 组织，鉴权热路径少一次 RTT。
    let (key, org) = api_keys::Entity::find()
        .filter(api_keys::Column::KeyHash.eq(&hash))
        .find_also_related(organizations::Entity)
        .one(db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    evaluate_key(&key, now).map_err(|e| match e {
        KeyError::Expired => AppError::Unauthorized,
        KeyError::Disabled => AppError::Forbidden,
        KeyError::Exhausted | KeyError::BudgetExceeded => AppError::QuotaExceeded,
    })?;

    let org = org.ok_or_else(|| AppError::Internal("organization missing for api_key".into()))?;
    if org.status != organizations::OrgStatus::Active {
        return Err(AppError::Forbidden);
    }

    Ok(KeyContext {
        api_key_id: key.id,
        org_id: org.id,
        user_id: key.user_id,
        group_id: org.group_id,
        allowed_models: key.allowed_models,
    })
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "identity ok" }))
        .route("/whoami", get(whoami))
}

/// `GET /api/identity/whoami`（Bearer 密钥）—— 校验并回显鉴权上下文（无密钥字段）。
async fn whoami(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<KeyContext>> {
    let raw = bearer(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;
    Ok(Json(ctx))
}

/// 从 Authorization 头解析 Bearer 原始令牌。方案名大小写不敏感（RFC 7235）。
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let val = headers.get(AUTHORIZATION)?.to_str().ok()?;
    // get(..7) 非 panic：长度不足或非字符边界返回 None
    if val.get(..7)?.eq_ignore_ascii_case("bearer ") {
        // 去空白并拒空，避免 "Bearer " 空 token 触发无意义查询
        let token = val[7..].trim();
        (!token.is_empty()).then_some(token)
    } else {
        None
    }
}
