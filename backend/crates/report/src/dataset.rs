//! 数据集端点：列表 / 详情 / 查询（核心）+ 内置数据集 seed。
//!
//! 数据集是策展语义层的对外契约；列表/详情按 principal 持有的权限点过滤（看不到无权数据集）。
//! 查询走 [`crate::engine::run`]，RLS 强制注入。
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::datasets;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

use crate::engine::{self, QueryReq, QueryResp};

/// `GET /api/report/datasets` —— 列出当前主体有权访问的数据集（按 required_permission 过滤）。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<datasets::Model>>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    if !principal.perms.contains("report.read") {
        return Err(AppError::Forbidden);
    }
    let db = state.db()?;
    // 按 required_permission ∈ 主体权限集 过滤下推 DB（避免全表载入内存）
    let my_perms: Vec<String> = principal.perms.iter().cloned().collect();
    let visible = datasets::Entity::find()
        .filter(datasets::Column::RequiredPermission.is_in(my_perms))
        .order_by_asc(datasets::Column::Id)
        .all(db)
        .await?;
    Ok(Json(visible))
}

/// `GET /api/report/datasets/{slug}` —— 数据集详情（含可用 metrics/dimensions，供前端构建器）。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> AppResult<Json<datasets::Model>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    let db = state.db()?;
    let ds = datasets::find_by_slug(db, &slug)
        .await?
        .ok_or(AppError::NotFound)?;
    if !principal.perms.contains(&ds.required_permission) {
        return Err(AppError::Forbidden);
    }
    Ok(Json(ds))
}

/// `POST /api/report/datasets/{slug}/query` —— 数据集查询（鉴权 + RLS 强制注入）。
pub async fn query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(req): Json<QueryReq>,
) -> AppResult<Json<QueryResp>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    let db = state.db()?;
    let ds = datasets::find_by_slug(db, &slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let resp = engine::run(&state, &principal, &ds, req).await?;
    Ok(Json(resp))
}

/// 内置数据集定义：(slug, name, source, metrics, dimensions, rls_rule, required_permission)。
/// 狗粮原则：内部报表与第三方走同一数据集契约。片A 仅落「用量」数据集打通端到端；
/// 业绩/账单/运维数据集在片B 增补（零引擎改动）。
fn builtin_datasets() -> Vec<(&'static str, &'static str, &'static str, serde_json::Value)> {
    vec![
        (
            "usage",
            "用量明细",
            "usage",
            serde_json::json!({
                "metrics": [
                    {"key": "calls", "label": "调用数"},
                    {"key": "revenue", "label": "消费(折后)"},
                    {"key": "avg_latency", "label": "平均延迟(ms)"}
                ],
                "dimensions": [
                    {"key": "model_id", "label": "模型"},
                    {"key": "channel_id", "label": "渠道"},
                    {"key": "day", "label": "日期"}
                ],
                // 客户仅见本组织；财务/运维/管理员全量；销售无分支（用量按归属需 JOIN，片B 增补）
                "rls_rule": {
                    "customer": {"column": "org_id", "param": "current_org"},
                    "finance": null,
                    "ops": null,
                    "admin": null
                },
                "required_permission": "report.read"
            }),
        ),
        (
            "billing",
            "账单明细",
            "orders",
            serde_json::json!({
                "metrics": [
                    {"key": "order_count", "label": "订单数"},
                    {"key": "order_amount", "label": "订单金额"},
                    {"key": "paid_amount", "label": "已支付金额"},
                    {"key": "paid_count", "label": "已支付订单数"}
                ],
                "dimensions": [
                    {"key": "status", "label": "订单状态"},
                    {"key": "pay_channel", "label": "支付渠道"},
                    {"key": "day", "label": "日期"}
                ],
                // 账单偏客户/财务：客户仅见本组织；财务/管理员全量。销售业绩走 sales_perf。
                "rls_rule": {
                    "customer": {"column": "org_id", "param": "current_org"},
                    "finance": null,
                    "admin": null
                },
                "required_permission": "report.read"
            }),
        ),
        (
            "sales_perf",
            "销售业绩",
            "orders",
            serde_json::json!({
                "metrics": [
                    {"key": "paid_amount", "label": "成交金额"},
                    {"key": "paid_count", "label": "成交订单数"},
                    {"key": "order_count", "label": "订单数(全状态)"},
                    {"key": "customer_count", "label": "成交客户数"}
                ],
                "dimensions": [
                    {"key": "created_by_sales_id", "label": "销售"},
                    {"key": "day", "label": "日期"},
                    {"key": "status", "label": "订单状态"}
                ],
                // 销售仅见本人名下（created_by_sales_id = 本人 user_id）；财务/管理员全量；客户无分支=禁止。
                "rls_rule": {
                    "sales": {"column": "created_by_sales_id", "param": "current_user"},
                    "finance": null,
                    "admin": null
                },
                "required_permission": "report.dataset.crm"
            }),
        ),
        (
            "channel_health",
            "渠道健康（运维）",
            "usage",
            serde_json::json!({
                // 运维不暴露 revenue
                "metrics": [
                    {"key": "calls", "label": "调用数"},
                    {"key": "avg_latency", "label": "平均延迟(ms)"},
                    {"key": "p95_latency", "label": "P95延迟(ms)"},
                    {"key": "stream_ratio", "label": "流式占比"}
                ],
                "dimensions": [
                    {"key": "channel_id", "label": "渠道"},
                    {"key": "model_id", "label": "模型"},
                    {"key": "day", "label": "日期"}
                ],
                // 运维/管理员全量；客户/销售/财务无分支=禁止。
                "rls_rule": {
                    "ops": null,
                    "admin": null
                },
                "required_permission": "report.dataset.ops"
            }),
        ),
    ]
}

/// 幂等 seed 内置数据集（按 slug 存在即跳过）。启动时调用，重放安全。
pub async fn seed_datasets(db: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    for (slug, name, src, spec) in builtin_datasets() {
        if datasets::find_by_slug(db, slug).await?.is_some() {
            continue;
        }
        datasets::ActiveModel {
            slug: Set(slug.to_owned()),
            name: Set(name.to_owned()),
            source: Set(src.to_owned()),
            metrics: Set(spec["metrics"].clone()),
            dimensions: Set(spec["dimensions"].clone()),
            rls_rule: Set(spec["rls_rule"].clone()),
            required_permission: Set(spec["required_permission"]
                .as_str()
                .unwrap_or("report.read")
                .to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }
    tracing::info!("report builtin datasets seeded");
    Ok(())
}
