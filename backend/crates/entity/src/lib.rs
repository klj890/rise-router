//! rise-entity —— 跨域共享的 SeaORM 实体。
//!
//! `models` 等实体被网关（路由）、定价、计费多域共用，集中于此避免域 crate 间循环依赖；
//! 业务逻辑仍各归其域 crate（如 `rise-pricing` 的 resolve_price）。

pub mod api_keys;
pub mod artifacts;
pub mod channels;
pub mod customer_assignments;
pub mod customer_notes;
pub mod datasets;
pub mod discounts;
pub mod groups;
pub mod invoices;
pub mod model_channels;
pub mod models;
pub mod orders;
pub mod organizations;
pub mod permissions;
pub mod phone_codes;
pub mod prices;
pub mod reconciliations;
pub mod report_definitions;
pub mod role_permissions;
pub mod roles;
pub mod tasks;
pub mod transactions;
pub mod usage_logs;
pub mod user_roles;
pub mod users;
pub mod wallets;
