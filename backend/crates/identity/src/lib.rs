//! 身份与组织域：organizations / users / groups / user_identities / 实名认证。
//!
//! M0：仅暴露占位路由 `/_ping`，验证域 crate 可独立编译并挂载到 server。
use axum::{routing::get, Router};
use rise_core::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "identity ok" }))
}
