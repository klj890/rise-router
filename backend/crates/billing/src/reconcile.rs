//! 应收侧对账（M2 片 C）。
//!
//! 按周期（月）聚合 usage_logs 营收（SUM charged_amount）+ 调用数 + 模型级明细，生成对账单。
//! 状态机 draft→locked：锁定即财务封账，只读，不可重算。
//! **全 admin 守卫**：对账是财务/运维侧跨 org 全量视图（非客户 RLS），与 recharge/orders 创建同一管理面。
//! 成本/毛利留后续片：usage_logs.cost_amount 恒 NULL（渠道成本字段未建），故 upstream_cost/gap 本片填 NULL。

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{TimeZone, Utc};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::reconciliations;
use rust_decimal::Decimal;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    Statement,
};
use serde::{Deserialize, Serialize};

use crate::admin_guard;

#[derive(Deserialize)]
pub struct GenerateReq {
    /// 对账周期，形如 "2026-06"（YYYY-MM）
    period: String,
}

/// 模型级明细一行。revenue 用 Decimal（serde 序列化为字符串，避免 f64 精度丢失）。
#[derive(Serialize)]
struct ModelLine {
    model_id: i32,
    revenue: Decimal,
    calls: i64,
}

/// 解析 period（YYYY-MM）→ [start, end) 左闭右开的 UTC 时间边界。
/// 非法格式（段数/位数/范围）返回 BadRequest。
fn period_bounds(period: &str) -> AppResult<(DateTimeWithTimeZone, DateTimeWithTimeZone)> {
    let bad = || AppError::BadRequest("period must be YYYY-MM".into());

    let (y, m) = period.split_once('-').ok_or_else(bad)?;
    if y.len() != 4
        || m.len() != 2
        || !y.bytes().all(|b| b.is_ascii_digit())
        || !m.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(bad());
    }
    let year: i32 = y.parse().map_err(|_| bad())?;
    let month: u32 = m.parse().map_err(|_| bad())?;
    if !(2000..=2100).contains(&year) || !(1..=12).contains(&month) {
        return Err(bad());
    }

    let start = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .ok_or_else(bad)?
        .fixed_offset();
    // 下月 1 号 0 点：12 月进位到次年 1 月
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = Utc
        .with_ymd_and_hms(ny, nm, 1, 0, 0, 0)
        .single()
        .ok_or_else(bad)?
        .fixed_offset();
    Ok((start, end))
}

/// `POST /api/billing/reconciliations`（admin 守卫）—— 生成/重算某周期对账单。
///
/// 幂等：周期不存在 → 建 draft；已存在 draft → 重算覆盖；已 locked → 400（封账只读）。
/// upstream_cost/gap 本片留 NULL（渠道成本未建）。
pub async fn generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GenerateReq>,
) -> AppResult<Json<reconciliations::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let (start, end) = period_bounds(&req.period)?;

    // locked 周期只读：先判定，避免白算一遍聚合再拒绝。
    let existing = reconciliations::Entity::find()
        .filter(reconciliations::Column::Period.eq(req.period.clone()))
        .one(db)
        .await?;
    if let Some(ref r) = existing {
        if r.status == reconciliations::ReconStatus::Locked {
            return Err(AppError::BadRequest(
                "locked period cannot be regenerated".into(),
            ));
        }
    }

    let backend = db.get_database_backend();

    // 总计：COALESCE 兜底空周期 NULL SUM；COUNT(*) 恒非 NULL。
    let total = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            "SELECT COALESCE(SUM(charged_amount), 0) AS revenue, COUNT(*) AS calls \
             FROM usage_logs WHERE created_at >= $1 AND created_at < $2",
            [start.into(), end.into()],
        ))
        .await?
        .ok_or_else(|| AppError::Internal("aggregate returned no row".into()))?;
    let total_revenue: Decimal = total.try_get("", "revenue")?;
    let total_calls: i64 = total.try_get("", "calls")?;

    // 模型级明细：GROUP BY model_id，按 revenue 倒序便于阅读。
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            backend,
            "SELECT model_id, COALESCE(SUM(charged_amount), 0) AS revenue, COUNT(*) AS calls \
             FROM usage_logs WHERE created_at >= $1 AND created_at < $2 \
             GROUP BY model_id ORDER BY revenue DESC",
            [start.into(), end.into()],
        ))
        .await?;
    let mut lines = Vec::with_capacity(rows.len());
    for row in &rows {
        lines.push(ModelLine {
            model_id: row.try_get("", "model_id")?,
            revenue: row.try_get("", "revenue")?,
            calls: row.try_get("", "calls")?,
        });
    }
    // 空周期 → []（非 NULL，区分「已对账无数据」与「未填」）。
    let detail = serde_json::to_value(&lines)
        .map_err(|e| AppError::Internal(format!("detail serialize: {e}")))?;

    let now = Utc::now().fixed_offset();

    let model = match existing {
        // 重算覆盖 draft：刷新营收/调用/明细/生成时间，status 保持 draft，cost/gap 仍 NULL。
        Some(r) => {
            let mut active: reconciliations::ActiveModel = r.into();
            active.total_revenue = Set(total_revenue);
            active.total_calls = Set(total_calls);
            active.upstream_cost = Set(None);
            active.gap = Set(None);
            active.detail = Set(Some(detail));
            active.generated_at = Set(now);
            active.update(db).await?
        }
        // 新建 draft。
        None => {
            reconciliations::ActiveModel {
                period: Set(req.period.clone()),
                status: Set(reconciliations::ReconStatus::Draft),
                total_revenue: Set(total_revenue),
                total_calls: Set(total_calls),
                upstream_cost: Set(None),
                gap: Set(None),
                detail: Set(Some(detail)),
                generated_at: Set(now),
                locked_at: Set(None),
                ..Default::default()
            }
            .insert(db)
            .await?
        }
    };

    Ok(Json(model))
}

/// `GET /api/billing/reconciliations`（admin 守卫）—— 列出所有对账单，period 倒序。
/// 财务全量视图，**不按 org 隔离**（对账本就是跨 org 聚合）。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<reconciliations::Model>>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let list = reconciliations::Entity::find()
        .order_by_desc(reconciliations::Column::Period)
        .all(db)
        .await?;
    Ok(Json(list))
}

/// `GET /api/billing/reconciliations/{id}`（admin 守卫）—— 单张对账单详情（含 detail）。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<reconciliations::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let r = reconciliations::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(r))
}

/// `POST /api/billing/reconciliations/{id}/lock`（admin 守卫）—— draft→locked 封账。
///
/// 条件 UPDATE（WHERE id=$ AND status=1）原子置 locked + locked_at；单条 UPDATE 即原子，无需事务。
/// 0 行 → 回查：不存在→404；已 locked→幂等返回（再次 lock 是安全 no-op）。
pub async fn lock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<reconciliations::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let at = Utc::now().fixed_offset();
    let backend = db.get_database_backend();

    // 仅 draft→locked 命中；并发下只有一个能拿到非空 RETURNING。
    let updated = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            "UPDATE reconciliations SET status = $1, locked_at = $2 \
             WHERE id = $3 AND status = $4 RETURNING id",
            [
                reconciliations::ReconStatus::Locked.into(),
                at.into(),
                id.into(),
                reconciliations::ReconStatus::Draft.into(),
            ],
        ))
        .await?;

    if updated.is_none() {
        // 0 行：不存在 → 404；已 locked → 幂等返回。
        let cur = reconciliations::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound)?;
        return Ok(Json(cur));
    }

    // 回查已封账对账单作响应。
    let r = reconciliations::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("reconciliation vanished after lock".into()))?;
    Ok(Json(r))
}
