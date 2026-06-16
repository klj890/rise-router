//! CRM 与销售域（M3）。
//!
//! 片 A：客户档案（org + 钱包 + 归属销售）、跟进记录、归属变更历史。
//!
//! **数据域隔离在端点层**（M3；完整 RLS 引擎留 M4）：销售（`crm.read`/`crm.write`，无
//! `crm.read.all`）仅见/操作自己名下客户（`owner_sales_id` = 本人）；管理员/财务（`crm.read.all`）
//! 与超管令牌见全部。统一经 [`rise_identity::require_scoped`] 决议 [`rise_identity::Access`]。
//!
//! **归属真相源**是 `organizations.owner_sales_id`；`customer_assignments` 记录变更轨迹（业绩归因）。
use axum::{
    routing::{get, post},
    Router,
};
use rise_core::AppState;

mod assignment;
mod customer;
mod note;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "crm ok" }))
        // 客户档案（org + 钱包余额 + 归属销售；数据域过滤）
        .route("/customers", get(customer::list))
        .route("/customers/{org_id}", get(customer::get_one))
        // 跟进记录（org 内倒序游标分页 + 新增）
        .route(
            "/customers/{org_id}/notes",
            get(note::list).post(note::create),
        )
        // 归属变更历史 + 改派
        .route("/customers/{org_id}/assignments", get(assignment::history))
        .route("/customers/{org_id}/assign", post(assignment::assign))
}
