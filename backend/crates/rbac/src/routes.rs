//! RBAC HTTP 路由。仅占位 `/_ping`：rbac 域保持"纯逻辑+实体"，不直接挂带鉴权的 HTTP；
//! 角色授予管理端点放在 identity 域（`identity::role_admin`，那里有 require/JWT），避免 rbac→identity 循环依赖。
use axum::{routing::get, Router};
use rise_core::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "rbac ok" }))
}
