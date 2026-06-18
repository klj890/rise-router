//! 销售代客充值（M3 片B）：销售为自己名下客户充值（线下/对公已收款 → 一步入账）。
//!
//! 数据域：经 [`load_scoped_org`] 校验——销售仅能给自己名下客户充值，越域 404 不泄露存在性。
//! 一步直接 Paid：事务内建 Paid 订单（`created_by_sales_id` = 操作者，供片C 业绩归因）
//! + `rise_billing::recharge` 入账（同事务 savepoint），原子一致。

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::orders;
use rust_decimal::Decimal;
use sea_orm::{ActiveModelTrait, Set, TransactionError, TransactionTrait};
use serde::{Deserialize, Serialize};

use crate::customer::load_scoped_org;

/// 单笔充值上限（与 `wallet::recharge` / numeric(18,8) 整数位对齐）。
const MAX_AMOUNT: i64 = 9_999_999_999;

#[derive(Deserialize)]
pub struct RechargeReq {
    amount: Decimal,
    /// 支付渠道（默认 transfer 对公；mock 阶段无真实支付）
    pay_channel: Option<String>,
    memo: Option<String>,
}

#[derive(Serialize)]
pub struct RechargeResp {
    order: orders::Model,
    /// 入账后余额
    balance: Decimal,
}

/// `POST /api/crm/customers/{org_id}/recharge`（crm.write + 数据域）—— 销售代客充值。
///
/// 数据域：销售仅能给自己名下客户充值（越域 404）。一步：事务内建 Paid 订单
/// （`created_by_sales_id` = 操作者）+ `rise_billing::recharge` 入账，原子提交。
pub async fn recharge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Json(req): Json<RechargeReq>,
) -> AppResult<Json<RechargeResp>> {
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.write", "crm.read.all").await?;
    let db = state.db()?;
    // 数据域：org 存在 + 归属校验（越域 404 不泄露存在性）
    load_scoped_org(db, org_id, &access).await?;

    // 金额校验（与 wallet::recharge 对齐：round 8 位后须 > 0 且 ≤ 上限）
    let amount = req.amount.round_dp(8);
    if amount <= Decimal::ZERO {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    if amount > Decimal::from(MAX_AMOUNT) {
        return Err(AppError::BadRequest("amount exceeds maximum limit".into()));
    }
    let pay_channel = req
        .pay_channel
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "transfer".to_owned());
    if pay_channel.chars().count() > 32 {
        return Err(AppError::BadRequest("pay_channel too long (max 32)".into()));
    }
    let created_by = access.actor_id(); // 操作者（超管令牌为 None）→ 业绩归因
    let memo = req
        .memo
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let now = chrono::Utc::now().fixed_offset();

    // 事务：建 Paid 订单 + 同事务入账（recharge 在 txn 内作 savepoint），原子——绝不建单不入账或反之。
    let resp = db
        .transaction::<_, RechargeResp, AppError>(move |txn| {
            Box::pin(async move {
                let order = orders::ActiveModel {
                    org_id: Set(org_id),
                    created_by_sales_id: Set(created_by),
                    amount: Set(amount),
                    pay_channel: Set(pay_channel),
                    trade_no: Set(None),
                    status: Set(orders::OrderStatus::Paid),
                    memo: Set(memo),
                    created_at: Set(now),
                    paid_at: Set(Some(now)),
                    ..Default::default()
                }
                .insert(txn)
                .await?;
                let balance = rise_billing::recharge_wallet(
                    txn,
                    org_id,
                    amount,
                    "order",
                    Some(order.id as i64),
                    None,
                    now,
                )
                .await?;
                Ok(RechargeResp { order, balance })
            })
        })
        .await
        .map_err(|e| match e {
            TransactionError::Connection(db) => AppError::Db(db),
            TransactionError::Transaction(app_err) => app_err,
        })?;

    tracing::info!(org_id, amount = %amount, "crm customer recharged by sales");
    Ok(Json(resp))
}
