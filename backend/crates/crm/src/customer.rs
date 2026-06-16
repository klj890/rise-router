//! 客户档案：列表 + 详情（组织信息 + 钱包余额快照 + 归属销售）。
//!
//! 数据域：销售（[`Access::owned_by`] = Some）仅见 `owner_sales_id` = 本人 的客户；
//! 管理员/财务/超管令牌见全部，可按 `owner_sales_id` 过滤查某销售名下客户。

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{organizations, wallets};
use rise_identity::Access;
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};

/// 客户档案视图：组织字段（flatten）+ 钱包余额/授信/冻结快照。
#[derive(Serialize)]
pub struct CustomerView {
    #[serde(flatten)]
    pub org: organizations::Model,
    /// 余额（无钱包 = 0）
    pub balance: Decimal,
    /// 授信额度
    pub credit_limit: Decimal,
    /// 冻结额
    pub frozen: Decimal,
}

impl CustomerView {
    fn build(org: organizations::Model, wallet: Option<&wallets::Model>) -> Self {
        Self {
            balance: wallet.map(|w| w.balance).unwrap_or(Decimal::ZERO),
            credit_limit: wallet.map(|w| w.credit_limit).unwrap_or(Decimal::ZERO),
            frozen: wallet.map(|w| w.frozen).unwrap_or(Decimal::ZERO),
            org,
        }
    }
}

/// 取客户组织并施加数据域：受限访问者（[`Access::owned_by`] = Some）只能取自己名下客户，
/// 否则返回 404（**不泄露存在性**——避免销售枚举他人客户 id）。其他域端点（notes/assignments）复用。
pub(crate) async fn load_scoped_org(
    db: &DatabaseConnection,
    org_id: i32,
    access: &Access,
) -> AppResult<organizations::Model> {
    let org = organizations::Entity::find_by_id(org_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(sales_id) = access.owned_by() {
        if org.owner_sales_id != Some(sales_id) {
            return Err(AppError::NotFound);
        }
    }
    Ok(org)
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// 可选按归属销售过滤（仅全量访问者生效；受限访问者强制本人名下）
    owner_sales_id: Option<i32>,
    /// 返回条数上限（默认 50，封顶 200）
    limit: Option<u64>,
    /// 游标：上一页最后一条 id；返回 id > cursor 的更大 id（id 升序）
    cursor: Option<i32>,
}

/// `GET /api/crm/customers`（crm.read[.all]）—— 客户列表，id 升序游标分页。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<CustomerView>>> {
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.read", "crm.read.all").await?;
    let db = state.db()?;

    let mut query = organizations::Entity::find();
    match access.owned_by() {
        // 受限：强制本人名下，忽略请求里的 owner_sales_id
        Some(sales_id) => {
            query = query.filter(organizations::Column::OwnerSalesId.eq(sales_id));
        }
        // 全量：可选按某销售过滤
        None => {
            if let Some(s) = q.owner_sales_id {
                query = query.filter(organizations::Column::OwnerSalesId.eq(s));
            }
        }
    }
    let limit = q.limit.unwrap_or(50).min(200);
    if let Some(cursor) = q.cursor {
        query = query.filter(organizations::Column::Id.gt(cursor));
    }
    let orgs = query
        .order_by_asc(organizations::Column::Id)
        .limit(limit)
        .all(db)
        .await?;

    // 批量取钱包，避免 N+1
    let org_ids: Vec<i32> = orgs.iter().map(|o| o.id).collect();
    let wallet_map: HashMap<i32, wallets::Model> = if org_ids.is_empty() {
        HashMap::new()
    } else {
        wallets::Entity::find()
            .filter(wallets::Column::OrgId.is_in(org_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|w| (w.org_id, w))
            .collect()
    };

    let views = orgs
        .into_iter()
        .map(|org| {
            let wallet = wallet_map.get(&org.id);
            CustomerView::build(org, wallet)
        })
        .collect();
    Ok(Json(views))
}

/// `GET /api/crm/customers/{org_id}`（crm.read[.all]）—— 客户详情。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
) -> AppResult<Json<CustomerView>> {
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.read", "crm.read.all").await?;
    let db = state.db()?;
    let org = load_scoped_org(db, org_id, &access).await?;
    let wallet = wallets::Entity::find()
        .filter(wallets::Column::OrgId.eq(org_id))
        .one(db)
        .await?;
    Ok(Json(CustomerView::build(org, wallet.as_ref())))
}
