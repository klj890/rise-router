//! rise-core —— 微内核：配置、数据库、错误、共享状态。
//!
//! 其余业务域 crate（identity/rbac/gateway/pricing/...）依赖此 crate 提供的
//! [`AppState`]、[`AppError`] 与连接工具，挂载到 `rise-server` 的 axum 路由上。

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod state;

pub use auth::{admin_guard, admin_token_ok};
pub use config::Config;
pub use error::{AppError, AppResult};
pub use state::AppState;
