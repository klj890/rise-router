//! 计费与财务域：usage_logs 结算 + 流水查询 + 钱包（余额/充值/消费）。orders/对账/发票留后续片。
//!
//! 纯算费在 [`charge`]（无 DB，单测覆盖）；[`settle`] 是结算编排，网关 relay 复用；
//! [`wallet`] 提供余额预检 + 充值入账 + 消费扣减（settle 复用）。
//! 「看流水/看余额」端点按密钥 org 隔离（RLS 雏形），走 Bearer 鉴权。

mod charge;
mod email;
mod email_cron;
mod export;
mod invoice;
mod margin;
mod order;
mod reconcile;
mod settle;
mod wallet;

pub use charge::{compute_charge, extract_token_usage};
pub use email_cron::spawn as spawn_email_cron;
pub use settle::{settle_chat, ChatSettlement};
pub use wallet::{ensure_funds, recharge as recharge_wallet, wallet_available};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{usage_logs, wallets};
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/margin", get(margin::margin))
        .route("/margin/export", get(export::export_margin))
        .route("/email/test", post(email_cron::email_test))
        .route("/_ping", get(|| async { "billing ok" }))
        .route("/usage", get(usage))
        .route("/wallet", get(wallet_get))
        .route("/recharge", post(recharge))
        .route("/orders", post(order::create_order).get(order::list_orders))
        .route("/orders/{id}/confirm", post(order::confirm_order))
        .route(
            "/reconciliations",
            post(reconcile::generate).get(reconcile::list),
        )
        .route("/reconciliations/{id}", get(reconcile::get_one))
        .route(
            "/reconciliations/{id}/export",
            get(export::export_reconciliation),
        )
        .route("/reconciliations/{id}/lock", post(reconcile::lock))
        .route("/invoices", post(invoice::create).get(invoice::list))
        .route("/invoices/{id}/issue", post(invoice::issue))
        .route("/invoices/{id}/void", post(invoice::void))
}

#[derive(Deserialize)]
struct UsageQuery {
    /// 返回条数上限（默认 50，封顶 200，避免拉全表）
    limit: Option<u64>,
    /// 游标：上一页最后一条记录的 id；返回 id < cursor 的更早记录（不传=最新一页）
    cursor: Option<i64>,
}

/// `GET /api/billing/usage`（Bearer 密钥）—— 看流水：仅返回本组织的调用计费明细，时间倒序。
/// 行级隔离按密钥归属的 org 强制过滤（RLS 雏形），客户碰不到他人流水。
async fn usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<UsageQuery>,
) -> AppResult<Json<Vec<usage_logs::Model>>> {
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    let limit = q.limit.unwrap_or(50).min(200);
    // 游标分页（按 id 倒序）：流水表无界增长，offset 深翻页要全扫前 N 行（性能），且高频追加会让
    // 行整体后移导致翻页重复（数据漂移）；游标走主键有序定位，耗时恒定且无漂移。id 单调 ≈ 时间序。
    let mut query = usage_logs::Entity::find().filter(usage_logs::Column::OrgId.eq(ctx.org_id));
    if let Some(cursor) = q.cursor {
        query = query.filter(usage_logs::Column::Id.lt(cursor));
    }
    let logs = query
        .order_by_desc(usage_logs::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    Ok(Json(logs))
}

#[derive(Serialize)]
struct WalletView {
    balance: Decimal,
    credit_limit: Decimal,
    frozen: Decimal,
    /// 可用额度 = balance + credit_limit − frozen
    available: Decimal,
}

/// `GET /api/billing/wallet`（Bearer 密钥）—— 看本组织钱包余额/授信/冻结/可用额度。无钱包返回全 0。
async fn wallet_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<WalletView>> {
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    let w = wallets::Entity::find()
        .filter(wallets::Column::OrgId.eq(ctx.org_id))
        .one(db)
        .await?;
    let view = match w {
        Some(w) => WalletView {
            available: w.balance + w.credit_limit - w.frozen,
            balance: w.balance,
            credit_limit: w.credit_limit,
            frozen: w.frozen,
        },
        None => WalletView {
            balance: Decimal::ZERO,
            credit_limit: Decimal::ZERO,
            frozen: Decimal::ZERO,
            available: Decimal::ZERO,
        },
    };
    Ok(Json(view))
}

#[derive(Deserialize)]
struct RechargeReq {
    org_id: i32,
    amount: Decimal,
    memo: Option<String>,
}

#[derive(Serialize)]
struct RechargeResp {
    org_id: i32,
    balance: Decimal,
}

/// `POST /api/billing/recharge`（管理令牌）—— 手动充值入账。
/// **临时守卫**：RBAC 落地前用 `X-Admin-Token` 头匹配 `RR_ADMIN_TOKEN`；未配置则禁用（403）。
/// 片 B 的订单支付成功后将复用底层 `wallet::recharge`，届时此端点收敛为管理后台用途。
async fn recharge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RechargeReq>,
) -> AppResult<Json<RechargeResp>> {
    rise_identity::require(&state, &headers, "billing.manage").await?;

    let db = state.db()?;
    // 客户端传入的 org_id 校验：不存在则 404（否则 ensure_wallet 的 INSERT 触发 FK 失败 → 500）
    if rise_entity::organizations::Entity::find_by_id(req.org_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }
    let now = chrono::Utc::now().fixed_offset();
    let balance =
        wallet::recharge(db, req.org_id, req.amount, "manual", None, req.memo, now).await?;
    Ok(Json(RechargeResp {
        org_id: req.org_id,
        balance,
    }))
}
