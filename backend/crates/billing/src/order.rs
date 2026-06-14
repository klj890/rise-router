//! 充值订单 + mock 支付（M2 片 B）。
//!
//! 创建订单（Pending，管理/销售令牌守卫）→ mock 确认（Pending→Paid，**幂等 + 原子入账**）。
//! 确认的原子性是片 A 把 `wallet::recharge` 泛型化成 `C: TransactionTrait` 的狗粮：
//! 「改订单状态」与「钱包入账 + 记 Recharge 流水」在同一事务内提交，绝不半成功。
//! 「看订单」端点按密钥 org 行级隔离（RLS 雏形），客户碰不到他人订单。

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::orders;
use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, Statement, TransactionError, TransactionTrait,
};
use serde::Deserialize;

use crate::admin_guard;

#[derive(Deserialize)]
pub struct CreateOrderReq {
    org_id: i32,
    amount: Decimal,
    pay_channel: String,
    created_by_sales_id: Option<i32>,
}

/// `POST /api/billing/orders`（管理令牌）—— 创建待支付订单。
/// 校验 org 存在（否则 404，避免 FK 失败 → 500）；amount round 到 8 位后须 > 0（否则 400）。
pub async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateOrderReq>,
) -> AppResult<Json<orders::Model>> {
    admin_guard(&state, &headers)?;

    let db = state.db()?;
    if rise_entity::organizations::Entity::find_by_id(req.org_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }

    // 与 numeric(18,8) 对齐后再校验：极小正数 round 到 0 不应建单
    let amount = req.amount.round_dp(8);
    if amount <= Decimal::ZERO {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    // 上限同 wallet::recharge：否则建单成功但确认时 recharge 拒 > 99 亿 → 订单永远无法确认
    if amount > Decimal::from(9_999_999_999i64) {
        return Err(AppError::BadRequest("amount exceeds maximum limit".into()));
    }

    let now = chrono::Utc::now().fixed_offset();
    let order = orders::ActiveModel {
        org_id: Set(req.org_id),
        created_by_sales_id: Set(req.created_by_sales_id),
        amount: Set(amount),
        pay_channel: Set(req.pay_channel),
        trade_no: Set(None),
        status: Set(orders::OrderStatus::Pending),
        memo: Set(None),
        created_at: Set(now),
        paid_at: Set(None),
        ..Default::default()
    };
    let order = order.insert(db).await?;
    Ok(Json(order))
}

#[derive(Deserialize)]
pub struct ConfirmReq {
    trade_no: Option<String>,
}

/// `POST /api/billing/orders/{id}/confirm`（管理令牌）—— **mock 支付确认**：Pending→Paid 并入账钱包。
///
/// 幂等 + 原子：
/// - 事务外先判定存在性/状态（DbErr 无法表达 404/400，故 404/幂等/400 留在事务外，语义清晰）：
///   不存在→404；已 Paid→幂等返回（不重复入账）；非 Pending（Failed/Refunded）→400。
/// - 仅 Pending 才进事务：事务内「条件 UPDATE（WHERE status=1，并发护栏）+ 同事务 recharge 入账」原子提交。
pub async fn confirm_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<ConfirmReq>,
) -> AppResult<Json<orders::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    // 事务外：存在性 + 幂等 + 状态判定（404/200 幂等/400），不进事务。
    let existing = orders::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    match existing.status {
        orders::OrderStatus::Paid => return Ok(Json(existing)), // 幂等：已付重复确认是安全 no-op
        orders::OrderStatus::Pending => {}                      // 唯一需要推进的状态，下落事务
        orders::OrderStatus::Failed | orders::OrderStatus::Refunded => {
            return Err(AppError::BadRequest("order is not payable".into()))
        }
    }

    let at = chrono::Utc::now().fixed_offset();
    let trade_no = req.trade_no.clone();

    // 事务内：条件 UPDATE（行锁 + WHERE status=Pending 并发护栏）+ 同事务 recharge 入账 → 原子提交。
    // 事务错误类型用 AppError：recharge 的 `?` 直接传播保留错误码（不被降级成 500）；状态用枚举参数不硬编码。
    let order = db
        .transaction::<_, orders::Model, AppError>(move |txn| {
            Box::pin(async move {
                let backend = txn.get_database_backend();
                let updated = txn
                    .query_one_raw(Statement::from_sql_and_values(
                        backend,
                        "UPDATE orders SET status = $1, paid_at = $2, trade_no = $3 \
                         WHERE id = $4 AND status = $5 RETURNING org_id, amount",
                        [
                            orders::OrderStatus::Paid.into(),
                            at.into(),
                            trade_no.into(),
                            id.into(),
                            orders::OrderStatus::Pending.into(),
                        ],
                    ))
                    .await?;

                match updated {
                    Some(row) => {
                        let org_id: i32 = row.try_get("", "org_id")?;
                        let amount: Decimal = row.try_get("", "amount")?;
                        // 同一 txn 内入账：改单状态 + 钱包 +amount + 记 Recharge 流水 原子一致（片 A 狗粮）。
                        crate::wallet::recharge(
                            txn,
                            org_id,
                            amount,
                            "order",
                            Some(id as i64),
                            None,
                            at,
                        )
                        .await?;
                    }
                    None => {
                        // 0 行：并发败者（另一事务已推进到 Paid）。回查幂等返回，不再入账。
                        let cur =
                            orders::Entity::find_by_id(id)
                                .one(txn)
                                .await?
                                .ok_or_else(|| {
                                    AppError::Internal("order vanished mid-confirm".into())
                                })?;
                        if cur.status != orders::OrderStatus::Paid {
                            return Err(AppError::Internal(
                                "order confirm race in bad state".into(),
                            ));
                        }
                        return Ok(cur);
                    }
                }

                // 回查已确认订单作响应。
                orders::Entity::find_by_id(id)
                    .one(txn)
                    .await?
                    .ok_or_else(|| AppError::Internal("order vanished after confirm".into()))
            })
        })
        .await
        .map_err(|e| match e {
            TransactionError::Connection(db) => AppError::Db(db),
            TransactionError::Transaction(app_err) => app_err,
        })?;

    Ok(Json(order))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// 返回条数上限（默认 50，封顶 200）
    limit: Option<u64>,
    /// 游标：上一页最后一条 id；返回 id < cursor 的更早订单
    cursor: Option<i32>,
}

/// `GET /api/billing/orders`（Bearer 密钥）—— 看本组织订单，id 倒序，游标分页（同 /usage）。
/// 行级隔离按密钥归属 org 强制过滤（RLS 雏形）。
pub async fn list_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<orders::Model>>> {
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    let limit = q.limit.unwrap_or(50).min(200);
    let mut query = orders::Entity::find().filter(orders::Column::OrgId.eq(ctx.org_id));
    if let Some(cursor) = q.cursor {
        query = query.filter(orders::Column::Id.lt(cursor));
    }
    let list = query
        .order_by_desc(orders::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    Ok(Json(list))
}
