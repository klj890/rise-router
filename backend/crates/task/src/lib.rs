//! 多模态任务域：tasks 状态机 / artifacts / 轮询 + webhook / 对象存储。
//!
//! M0：仅暴露占位路由 `/_ping`，验证域 crate 可独立编译并挂载到 server。
use axum::{routing::get, Router};
use rise_core::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "task ok" }))
}
