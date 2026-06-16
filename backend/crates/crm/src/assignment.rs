//! 客户归属变更（改派给某销售）+ 变更历史。
//!
//! 归属是管理动作（`crm.assign`，管理员级，无归属边界）。改派在**单事务**内完成：
//! 关闭旧 active 行 → 插入新 active 行 → 更新 `organizations.owner_sales_id`（真相源），原子一致。
//! 幂等：目标销售已是当前归属则 no-op（不写重复历史）。

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{customer_assignments, organizations, users};
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionError, TransactionTrait,
};
use serde::Deserialize;

/// `GET /api/crm/customers/{org_id}/assignments`（crm.read[.all]）—— 归属变更历史，id 倒序。
pub async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
) -> AppResult<Json<Vec<customer_assignments::Model>>> {
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.read", "crm.read.all").await?;
    let db = state.db()?;
    crate::customer::load_scoped_org(db, org_id, &access).await?; // 数据域校验

    let rows = customer_assignments::Entity::find()
        .filter(customer_assignments::Column::OrgId.eq(org_id))
        .order_by_desc(customer_assignments::Column::Id)
        .all(db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct AssignReq {
    /// 改派到的销售（users.id，必须存在）
    sales_id: i32,
}

/// `POST /api/crm/customers/{org_id}/assign`（crm.assign，管理员级）—— 改派客户归属。
///
/// 事务外：org 存在性（404）+ 目标销售存在性（400，软引用防幽灵）+ 幂等判定（已是当前归属 → 200 no-op）。
/// 事务内：关旧 active 行 + 插新 active 行 + 改 owner_sales_id，原子提交。
pub async fn assign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Json(req): Json<AssignReq>,
) -> AppResult<Json<organizations::Model>> {
    rise_identity::require(&state, &headers, "crm.assign").await?;
    let db = state.db()?;

    let org = organizations::Entity::find_by_id(org_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    // 校验目标销售存在（软引用，避免 owner_sales_id 指向幽灵 user）
    if users::Entity::find_by_id(req.sales_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::BadRequest("sales_id not found".into()));
    }
    // 幂等：已是当前归属 → no-op（不写重复历史）
    if org.owner_sales_id == Some(req.sales_id) {
        return Ok(Json(org));
    }

    let now = chrono::Utc::now().fixed_offset();
    let sales_id = req.sales_id;
    let updated = db
        .transaction::<_, organizations::Model, AppError>(move |txn| {
            Box::pin(async move {
                // 1. 关闭该 org 的旧 active 归属行
                customer_assignments::Entity::update_many()
                    .col_expr(customer_assignments::Column::Active, Expr::value(false))
                    .filter(customer_assignments::Column::OrgId.eq(org_id))
                    .filter(customer_assignments::Column::Active.eq(true))
                    .exec(txn)
                    .await?;
                // 2. 插入新 active 归属行（变更轨迹）
                customer_assignments::ActiveModel {
                    org_id: Set(org_id),
                    sales_id: Set(sales_id),
                    assigned_at: Set(now),
                    active: Set(true),
                    ..Default::default()
                }
                .insert(txn)
                .await?;
                // 3. 更新真相源 organizations.owner_sales_id
                let mut am: organizations::ActiveModel = org.into();
                am.owner_sales_id = Set(Some(sales_id));
                let m = am.update(txn).await?;
                Ok(m)
            })
        })
        .await
        .map_err(|e| match e {
            TransactionError::Connection(db) => AppError::Db(db),
            TransactionError::Transaction(app_err) => app_err,
        })?;

    tracing::info!(org_id, sales_id, "crm customer reassigned");
    Ok(Json(updated))
}
