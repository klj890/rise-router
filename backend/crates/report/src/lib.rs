//! 监控报表域（M4）：策展数据集 + 行级隔离(RLS)查询引擎 + 定制报表定义。
//!
//! **不开放原始库**：管理员定义数据集（[`source`] 白名单 + 策展 metrics/dimensions + 按角色
//! rls_rule），报表只能基于数据集；查询时 [`engine`] 按当前用户角色强制注入行级过滤，用户无法绕过。
//! 复用 RBAC 与 [`rise_identity::Principal`]。
//!
//! 片A（本片）：内核——两表 + 引擎 + datasets/reports 端点 + 内置「用量」数据集打通端到端。
//! 片B：业绩/账单/运维数据集（零引擎改动）。片C：前端报表构建器。
use axum::{
    routing::{get, post},
    Router,
};
use rise_core::AppState;

mod dataset;
mod engine;
mod report_def;
mod source;

pub use dataset::seed_datasets;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "report ok" }))
        // 数据集：列表 / 详情 / 查询（RLS 强制注入）
        .route("/datasets", get(dataset::list))
        .route("/datasets/{slug}", get(dataset::get_one))
        .route("/datasets/{slug}/query", post(dataset::query))
        // 定制报表定义：列表 / 创建 / 详情 / 删除
        .route("/reports", get(report_def::list).post(report_def::create))
        .route(
            "/reports/{id}",
            get(report_def::get_one).delete(report_def::delete),
        )
}
