//! 财务控制台跨租户只读视图（`billing.read`）。
//!
//! 与按密钥 org 隔离的 `usage`/`wallet`/`orders`/`invoices` 不同：这些端点面向财务/管理员，
//! **跨全部组织**聚合，供前端计费页的「账单总览 / 租户用量 / 充值记录 / 发票」展示。
//! 鉴权走 [`rise_identity::require`]（用户 JWT+RBAC 或超管令牌逃生通道）。

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppResult, AppState};
use rise_entity::{invoices, orders, organizations, usage_logs, wallets};
use rust_decimal::Decimal;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, EntityTrait, ExprTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect,
};
use serde::{Deserialize, Serialize};

use crate::margin::period_range;

/// 取指定组织 id→名称映射（仅查本页涉及的 org，避免全表扫描）。空列表直接返回空表。
async fn org_names(db: &sea_orm::DatabaseConnection, ids: &[i32]) -> AppResult<HashMap<i32, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let orgs = organizations::Entity::find()
        .filter(organizations::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    Ok(orgs.into_iter().map(|o| (o.id, o.name)).collect())
}

#[derive(Deserialize)]
pub struct PeriodQuery {
    /// 周期 YYYY-MM；缺省 = 当月（UTC）
    period: Option<String>,
}

#[derive(Deserialize)]
pub struct LimitQuery {
    /// 返回条数上限（默认 100，封顶 500）
    limit: Option<u64>,
}

/// 租户用量行：本周期调用数 + 消费 + 当前钱包余额。
#[derive(Serialize)]
pub struct TenantRow {
    org_id: i32,
    org_name: String,
    calls: i64,
    charged: Decimal,
    balance: Decimal,
}

#[derive(FromQueryResult)]
struct TenantAgg {
    org_id: i32,
    calls: i64,
    charged: Option<Decimal>,
}

/// `GET /api/billing/admin/tenants?period=YYYY-MM`（billing.read）
///
/// 跨租户用量总览：按 org 聚合本周期调用数与消费额，并联当前钱包余额，消费倒序。
pub async fn tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PeriodQuery>,
) -> AppResult<Json<Vec<TenantRow>>> {
    rise_identity::require(&state, &headers, "billing.read").await?;
    let db = state.db()?;
    let ((start, end), _period) = period_range(q.period)?;

    let aggs = usage_logs::Entity::find()
        .filter(usage_logs::Column::CreatedAt.gte(start))
        .filter(usage_logs::Column::CreatedAt.lt(end))
        .select_only()
        .column(usage_logs::Column::OrgId)
        .column_as(Expr::col(usage_logs::Column::Id).count(), "calls")
        .column_as(
            Expr::col(usage_logs::Column::ChargedAmount).sum(),
            "charged",
        )
        .group_by(usage_logs::Column::OrgId)
        .into_model::<TenantAgg>()
        .all(db)
        .await?;

    // 仅查本期活跃 org 的名称与钱包，避免随租户总数线性增长的全表扫描。
    let org_ids: Vec<i32> = aggs.iter().map(|a| a.org_id).collect();
    let names = org_names(db, &org_ids).await?;
    let balances: HashMap<i32, Decimal> = if org_ids.is_empty() {
        HashMap::new()
    } else {
        wallets::Entity::find()
            .filter(wallets::Column::OrgId.is_in(org_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|w| (w.org_id, w.balance))
            .collect()
    };

    let mut rows: Vec<TenantRow> = aggs
        .into_iter()
        .map(|a| TenantRow {
            org_name: names
                .get(&a.org_id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", a.org_id)),
            calls: a.calls,
            charged: a.charged.unwrap_or(Decimal::ZERO),
            balance: balances.get(&a.org_id).copied().unwrap_or(Decimal::ZERO),
            org_id: a.org_id,
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.charged));
    Ok(Json(rows))
}

/// 充值订单行（跨租户）：订单字段 + 租户名。
#[derive(Serialize)]
pub struct OrderRow {
    #[serde(flatten)]
    order: orders::Model,
    org_name: String,
}

/// `GET /api/billing/admin/orders?limit=`（billing.read）—— 跨租户充值订单，按 id 倒序。
pub async fn orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LimitQuery>,
) -> AppResult<Json<Vec<OrderRow>>> {
    rise_identity::require(&state, &headers, "billing.read").await?;
    let db = state.db()?;
    let limit = core::cmp::min(q.limit.unwrap_or(100), 500);

    let list = orders::Entity::find()
        .order_by_desc(orders::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    // 去重 org_id：同一组织可能有多笔订单，避免 IN 查询冗余。
    let mut org_ids: Vec<i32> = list.iter().map(|o| o.org_id).collect();
    org_ids.sort_unstable();
    org_ids.dedup();
    let names = org_names(db, &org_ids).await?;
    let rows = list
        .into_iter()
        .map(|o| OrderRow {
            org_name: names
                .get(&o.org_id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", o.org_id)),
            order: o,
        })
        .collect();
    Ok(Json(rows))
}

/// 发票行（跨租户）：发票字段 + 租户名。
#[derive(Serialize)]
pub struct InvoiceRow {
    #[serde(flatten)]
    invoice: invoices::Model,
    org_name: String,
}

/// `GET /api/billing/admin/invoices?limit=`（billing.read）—— 跨租户发票，按 id 倒序。
pub async fn invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LimitQuery>,
) -> AppResult<Json<Vec<InvoiceRow>>> {
    rise_identity::require(&state, &headers, "billing.read").await?;
    let db = state.db()?;
    let limit = core::cmp::min(q.limit.unwrap_or(100), 500);

    let list = invoices::Entity::find()
        .order_by_desc(invoices::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    // 去重 org_id：同一组织可能有多张发票，避免 IN 查询冗余。
    let mut org_ids: Vec<i32> = list.iter().map(|i| i.org_id).collect();
    org_ids.sort_unstable();
    org_ids.dedup();
    let names = org_names(db, &org_ids).await?;
    let rows = list
        .into_iter()
        .map(|i| InvoiceRow {
            org_name: names
                .get(&i.org_id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", i.org_id)),
            invoice: i,
        })
        .collect();
    Ok(Json(rows))
}
