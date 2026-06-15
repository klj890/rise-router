//! 身份与组织域：organizations / api_keys / users / 密钥鉴权（verify_key）+ 用户会话（手机号短信登录）。
//!
//! 纯校验在 [`auth`]（无 DB，单测覆盖）；[`verify_key`] 是 DB 编排，relay 鉴权复用。
//! 用户注册/登录（手机号 + 短信验证码 + JWT 会话）在 [`session`]，与 api_key 鉴权两条独立路径。

mod api_key;
mod auth;
mod organization;
mod session;

pub use auth::{evaluate_key, hash_key, KeyContext, KeyError};

use axum::{
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap},
    routing::{get, post},
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
        // 用户会话：手机号 + 短信验证码注册/登录（公开）+ /me（用户 JWT）
        .route("/auth/send-code", post(session::send_code))
        .route("/auth/login", post(session::login))
        .route("/me", get(session::me))
        // 组织管理 CRUD（admin 守卫）
        .route(
            "/organizations",
            post(organization::create).get(organization::list),
        )
        .route(
            "/organizations/{id}",
            get(organization::get_one)
                .put(organization::update)
                .delete(organization::delete),
        )
        // 虚拟密钥管理 CRUD（admin 守卫；创建明文仅回显一次）
        .route("/api-keys", post(api_key::create).get(api_key::list))
        .route(
            "/api-keys/{id}",
            get(api_key::get_one)
                .put(api_key::update)
                .delete(api_key::delete),
        )
}

/// `GET /api/identity/whoami`（Bearer 密钥）—— 校验并回显鉴权上下文（无密钥字段）。
async fn whoami(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<KeyContext>> {
    let raw = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;
    Ok(Json(ctx))
}

/// 从 Authorization 头解析 Bearer 原始令牌。方案名大小写不敏感（RFC 7235）。
/// 供 relay 等其他域复用，避免重复实现。
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
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
