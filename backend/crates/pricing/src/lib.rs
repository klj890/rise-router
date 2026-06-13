//! 定价域：prices / discounts / resolve_price() 纯函数（查表 + 折扣叠加）。
//!
//! M0：仅暴露占位路由 `/_ping`，验证域 crate 可独立编译并挂载到 server。
use axum::{routing::get, Router};
use rise_core::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "pricing ok" }))
}
