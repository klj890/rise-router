//! 发票申请与开票（M2 片 D）。
//!
//! 客户对已充值金额申请开票（Bearer 密钥，org 取自 ctx 不信任客户端）→ Pending；
//! 财务推进开票（admin 守卫）Pending→Issued，或作废 →Voided。
//! 区分普票/专票（专票必须有税号）。order_id 软引用某笔充值订单（不建 FK，发票独立留存）。
//! issue/void 走单条条件 UPDATE + RETURNING（原子 + 并发护栏 + 幂等），与 reconcile::lock 同构。

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::invoices;
use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, QuerySelect, Set, Statement,
};
use serde::Deserialize;

use crate::admin_guard;

#[derive(Deserialize)]
pub struct CreateReq {
    /// 软引用某笔充值订单（可选，为某笔充值开票）
    order_id: Option<i32>,
    /// 发票类型：1=普票 2=专票（映射枚举不硬编码数字）
    invoice_type: i16,
    /// 发票抬头
    title: String,
    /// 纳税人识别号（专票必填）
    tax_no: Option<String>,
    /// 开票金额（客户自报，round 到 8 位后须 > 0）
    amount: Decimal,
    memo: Option<String>,
}

/// 把请求体的 i16 映射到强类型枚举；非法值 → 400（不硬编码数字落库）。
fn parse_invoice_type(v: i16) -> AppResult<invoices::InvoiceType> {
    match v {
        1 => Ok(invoices::InvoiceType::Normal),
        2 => Ok(invoices::InvoiceType::Special),
        _ => Err(AppError::BadRequest(
            "invoice_type must be 1 (normal) or 2 (special)".into(),
        )),
    }
}

/// `POST /api/billing/invoices`（Bearer 密钥）—— 客户为自己 org 申请开票，建 Pending 发票。
///
/// org_id 取自密钥归属 org（**不信任客户端**，客户只能给自己开）。
/// 校验：amount round_dp(8) 且 0 < amount ≤ 99 亿；title 非空且 ≤128；专票必须有 tax_no；
/// tax_no ≤64；memo ≤256；invoice_type 合法。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<invoices::Model>> {
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    let invoice_type = parse_invoice_type(req.invoice_type)?;

    // 与 numeric(18,8) 对齐后再校验：极小正数 round 到 0 不应建发票。
    let amount = req.amount.round_dp(8);
    if amount <= Decimal::ZERO {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    // 上限同 wallet/order：99 亿。
    if amount > Decimal::from(9_999_999_999i64) {
        return Err(AppError::BadRequest("amount exceeds maximum limit".into()));
    }

    // title 列 varchar(128)：非空 + 超长先拦 400（防 500）。
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    if title.chars().count() > 128 {
        return Err(AppError::BadRequest("title too long (max 128)".into()));
    }

    // tax_no 列 varchar(64)：超长先拦 400。空白视同未填。
    let tax_no = req
        .tax_no
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Some(ref t) = tax_no {
        if t.chars().count() > 64 {
            return Err(AppError::BadRequest("tax_no too long (max 64)".into()));
        }
    }
    // 专票必须有税号（税务认证抵扣前置）。
    if invoice_type == invoices::InvoiceType::Special && tax_no.is_none() {
        return Err(AppError::BadRequest(
            "tax_no is required for special invoice".into(),
        ));
    }

    // memo 列 varchar(256)：超长先拦 400。
    if let Some(ref m) = req.memo {
        if m.chars().count() > 256 {
            return Err(AppError::BadRequest("memo too long (max 256)".into()));
        }
    }

    let now = chrono::Utc::now().fixed_offset();
    let invoice = invoices::ActiveModel {
        org_id: Set(ctx.org_id),
        order_id: Set(req.order_id),
        invoice_type: Set(invoice_type),
        title: Set(title.to_owned()),
        tax_no: Set(tax_no),
        amount: Set(amount),
        status: Set(invoices::InvoiceStatus::Pending),
        memo: Set(req.memo),
        created_at: Set(now),
        issued_at: Set(None),
        ..Default::default()
    };
    let invoice = invoice.insert(db).await?;
    Ok(Json(invoice))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// 返回条数上限（默认 50，封顶 200）
    limit: Option<u64>,
    /// 游标：上一页最后一条 id；返回 id < cursor 的更早发票
    cursor: Option<i32>,
}

/// `GET /api/billing/invoices`（Bearer 密钥）—— 看本组织发票，id 倒序，游标分页。
/// 行级隔离按密钥归属 org 强制过滤（RLS 雏形）。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<invoices::Model>>> {
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    let limit = q.limit.unwrap_or(50).min(200);
    let mut query = invoices::Entity::find().filter(invoices::Column::OrgId.eq(ctx.org_id));
    if let Some(cursor) = q.cursor {
        query = query.filter(invoices::Column::Id.lt(cursor));
    }
    let list = query
        .order_by_desc(invoices::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    Ok(Json(list))
}

/// `POST /api/billing/invoices/{id}/issue`（admin 守卫）—— 财务开票：Pending→Issued。
///
/// 条件 UPDATE（WHERE id=$ AND status=Pending）原子置 Issued + issued_at，RETURNING * 直接映射 Model。
/// 0 行回查：不存在→404；已 Issued→幂等返回；Voided→400（作废的发票不能再开）。
pub async fn issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<invoices::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let at = chrono::Utc::now().fixed_offset();
    let backend = db.get_database_backend();

    // 仅 Pending→Issued 命中；并发下只有一个能拿到非空 RETURNING。
    let updated = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            "UPDATE invoices SET status = $1, issued_at = $2 \
             WHERE id = $3 AND status = $4 RETURNING *",
            [
                invoices::InvoiceStatus::Issued.into(),
                at.into(),
                id.into(),
                invoices::InvoiceStatus::Pending.into(),
            ],
        ))
        .await?;

    match updated {
        Some(row) => Ok(Json(invoices::Model::from_query_result(&row, "")?)),
        None => {
            // 0 行：不存在 → 404；已 Issued → 幂等返回；Voided → 400。
            let cur = invoices::Entity::find_by_id(id)
                .one(db)
                .await?
                .ok_or(AppError::NotFound)?;
            match cur.status {
                invoices::InvoiceStatus::Issued => Ok(Json(cur)), // 幂等：已开重复开是安全 no-op
                invoices::InvoiceStatus::Voided => Err(AppError::BadRequest(
                    "voided invoice cannot be issued".into(),
                )),
                // Pending：理论被并发改回（不会发生），保守按 400 防止静默吞错。
                invoices::InvoiceStatus::Pending => {
                    Err(AppError::BadRequest("invoice is not issuable".into()))
                }
            }
        }
    }
}

/// `POST /api/billing/invoices/{id}/void`（admin 守卫）—— 作废发票 →Voided。
///
/// 条件 UPDATE（WHERE id=$ AND status != Voided）原子置 Voided，允许 Pending/Issued→Voided。
/// RETURNING * 直接映射 Model。0 行回查：不存在→404；已 Voided→幂等返回。
pub async fn void(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<invoices::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let backend = db.get_database_backend();

    // status != Voided 命中（Pending/Issued 均可作废）；并发下只有一个能拿到非空 RETURNING。
    let updated = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            "UPDATE invoices SET status = $1 \
             WHERE id = $2 AND status <> $3 RETURNING *",
            [
                invoices::InvoiceStatus::Voided.into(),
                id.into(),
                invoices::InvoiceStatus::Voided.into(),
            ],
        ))
        .await?;

    match updated {
        Some(row) => Ok(Json(invoices::Model::from_query_result(&row, "")?)),
        None => {
            // 0 行：不存在 → 404；已 Voided → 幂等返回。
            let cur = invoices::Entity::find_by_id(id)
                .one(db)
                .await?
                .ok_or(AppError::NotFound)?;
            Ok(Json(cur)) // 此时 cur.status 必为 Voided（唯一被 WHERE 排除的态）：幂等返回
        }
    }
}
