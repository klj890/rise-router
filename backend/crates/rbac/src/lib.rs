//! RBAC 域：roles / permissions / role_permissions / user_roles / enforce(sub,dom,obj,act)。
//!
//! M0：仅暴露占位路由 `/_ping`，验证域 crate 可独立编译并挂载到 server。
use axum::{routing::get, Router};
use rise_core::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "rbac ok" }))
}
