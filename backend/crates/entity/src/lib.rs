//! rise-entity —— 跨域共享的 SeaORM 实体。
//!
//! `models` 等实体被网关（路由）、定价、计费多域共用，集中于此避免域 crate 间循环依赖；
//! 业务逻辑仍各归其域 crate（如 `rise-pricing` 的 resolve_price）。

pub mod api_keys;
pub mod channels;
pub mod discounts;
pub mod groups;
pub mod model_channels;
pub mod models;
pub mod organizations;
pub mod prices;
pub mod usage_logs;
