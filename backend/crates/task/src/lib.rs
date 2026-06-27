//! 多模态任务域（M5a）：统一 `/v1/tasks` 提交/查询/取消 + 状态机 + 队列 + 运行时。
//!
//! - `v1_routes()` 挂根路径（OpenAI 风格 `/v1/tasks`，与 `/v1/chat/completions` 同层）。
//! - `routes()` 留 `/api/task`（内部/管理用，暂 `_ping`）。
//! - `spawn_task_runtime()`：worker（队列消费 + 提交上游）+ poller（扫 running 续查，可恢复）。
use axum::{
    routing::{get, post},
    Router,
};
use rise_core::AppState;

/// 任务式厂商适配器扩展点（新增真实厂商在此实现 `TaskAdapter` 并注册到 `adapter_for`）。
pub mod adapter;
mod api;
mod worker;

pub use worker::spawn_task_runtime;

/// 任务队列（Redis list）键：submit 时 LPUSH 任务 id，worker BRPOPLPUSH 消费（片B）。
pub const QUEUE_KEY: &str = "rr:tasks:queued";
/// worker 取出后暂存的处理中列表（崩溃恢复用，片B/C）。
pub const PROCESSING_KEY: &str = "rr:tasks:processing";

/// 域内部/管理路由（挂 `/api/task`）。
pub fn routes() -> Router<AppState> {
    Router::new().route("/_ping", get(|| async { "task ok" }))
}

/// 对外统一任务 API（挂根 `/v1`，Bearer 密钥鉴权 + org 行隔离）。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/tasks", post(api::submit))
        .route("/v1/tasks/{id}", get(api::get_one))
        .route("/v1/tasks/{id}/cancel", post(api::cancel))
}
