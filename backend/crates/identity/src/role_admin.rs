//! 角色授予管理（RBAC 接线）：列角色 / 权限点、查 / 授 / 撤用户角色。
//!
//! 守卫 `require("rbac.manage")`（admin 与超管令牌持有者可用）。放在 identity 域而非 rbac——
//! 因为需要 require（含 JWT 校验），而 require 在 identity；放 rbac 会造成 rbac→identity 循环依赖。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{permissions, roles, users};
use sea_orm::EntityTrait;
use serde::Deserialize;

const PERM: &str = "rbac.manage";

/// `GET /api/identity/roles` —— 列出全部角色。
pub async fn list_roles(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<roles::Model>>> {
    crate::require(&state, &headers, PERM).await?;
    let db = state.db()?;
    Ok(Json(rise_rbac::list_roles(db).await?))
}

/// `GET /api/identity/permissions` —— 列出全部权限点目录。
pub async fn list_permissions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<permissions::Model>>> {
    crate::require(&state, &headers, PERM).await?;
    let db = state.db()?;
    Ok(Json(rise_rbac::list_permissions(db).await?))
}

/// `GET /api/identity/users/{id}/roles` —— 查某用户已授角色。
pub async fn list_user_roles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
) -> AppResult<Json<Vec<roles::Model>>> {
    crate::require(&state, &headers, PERM).await?;
    let db = state.db()?;
    Ok(Json(rise_rbac::list_user_roles(db, user_id).await?))
}

#[derive(Deserialize)]
pub struct GrantReq {
    role_slug: String,
}

/// `POST /api/identity/users/{id}/roles` —— 给用户授角色（幂等）。返回该用户最新角色集。
pub async fn grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<i32>,
    Json(req): Json<GrantReq>,
) -> AppResult<Json<Vec<roles::Model>>> {
    crate::require(&state, &headers, PERM).await?;
    let db = state.db()?;
    let slug = req.role_slug.trim();
    if !rise_rbac::ROLES.iter().any(|(s, _)| *s == slug) {
        return Err(AppError::BadRequest(format!("unknown role '{slug}'")));
    }
    // 角色行须确已落库：grant_role 在角色缺失时静默 no-op，若 seed 未跑（启动时 DB 不可用仅 warn）
    // 会"授权成功却没写入"。显式校验 DB 角色存在，缺失则 503，避免静默丢授权。
    if roles::find_by_slug(db, slug).await?.is_none() {
        return Err(AppError::Unavailable);
    }
    // 用户必须存在（user_roles FK；否则 insert 触发 FK 失败 → 500）。
    if users::Entity::find_by_id(user_id).one(db).await?.is_none() {
        return Err(AppError::BadRequest("user not found".into()));
    }
    rise_rbac::grant_role(db, user_id, slug).await?;
    Ok(Json(rise_rbac::list_user_roles(db, user_id).await?))
}

/// `DELETE /api/identity/users/{id}/roles/{role_slug}` —— 撤销用户某角色（幂等）。
pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((user_id, role_slug)): Path<(i32, String)>,
) -> AppResult<StatusCode> {
    crate::require(&state, &headers, PERM).await?;
    let db = state.db()?;
    rise_rbac::revoke_role(db, user_id, &role_slug).await?;
    Ok(StatusCode::NO_CONTENT)
}
