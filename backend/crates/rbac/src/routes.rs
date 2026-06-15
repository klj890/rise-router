//! RBAC HTTP 路由。当前仅占位 `/_ping`；角色/权限点/用户授角色的管理 CRUD 下一片接入。
use axum::{routing::get, Router};
use rise_core::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "rbac ok" }))
}
