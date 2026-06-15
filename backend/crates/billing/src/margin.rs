//! 毛利报表：按周期聚合 Σcharged − Σcost = 毛利，可按 model/channel 维度下钻。
//!
//! 成本取自 `usage_logs.cost_amount`（结算时由渠道成本价 `model_channels.cost_price`
//! 按同量纲算得）。部分行可能无成本（渠道未配成本价）→ `cost_amount` NULL，SUM/COUNT 忽略；
//! `cost_complete` 标志诚实反映毛利可信度（false = 有行缺成本，毛利偏乐观，未配成本按 0 计）。
//!
//! 五要素解耦：成本走路由线（model_channels），售价走定价线（prices），毛利只是二者在
//! usage_logs 上的事后差额聚合，不引入新的耦合。

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{Datelike, TimeZone, Utc};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::usage_logs;
use rust_decimal::Decimal;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, EntityTrait, ExprTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct MarginQuery {
    /// 周期 YYYY-MM；缺省 = 当月（UTC）
    period: Option<String>,
    /// 下钻维度：`model` | `channel`；缺省 = 总览单行
    group_by: Option<String>,
}

/// 总览聚合行（无维度）。
#[derive(FromQueryResult)]
struct OverviewRow {
    revenue: Option<Decimal>,
    cost: Option<Decimal>,
    total_calls: i64,
    cost_covered_calls: i64,
}

/// 分组聚合行（带维度 id）。
#[derive(FromQueryResult)]
struct GroupRow {
    dim_id: i32,
    revenue: Option<Decimal>,
    cost: Option<Decimal>,
    total_calls: i64,
    cost_covered_calls: i64,
}

#[derive(Serialize)]
struct MarginCell {
    /// 维度标识（如 `model:3` / `channel:5`）；总览为 null
    dim: Option<String>,
    dim_id: Option<i32>,
    /// 营收 = Σ charged_amount（全部计费行）
    revenue: Decimal,
    /// 成本 = Σ cost_amount（仅已配成本价的行）
    cost: Decimal,
    /// 毛利 = revenue − cost
    gross_profit: Decimal,
    /// 毛利率 = 毛利 / 营收；营收为 0 时 null
    margin_rate: Option<Decimal>,
    total_calls: i64,
    /// 含成本（cost_amount 非空）的调用数
    cost_covered_calls: i64,
}

#[derive(Serialize)]
pub struct MarginResp {
    period: String,
    group_by: Option<String>,
    /// 该周期参与聚合的计费行是否都已配成本价（cost_covered == total）→ 毛利完整可信
    cost_complete: bool,
    rows: Vec<MarginCell>,
}

/// `YYYY-MM` → [当月首日, 次月首日) 的 UTC 半开区间。
fn parse_period(period: &str) -> AppResult<(DateTimeWithTimeZone, DateTimeWithTimeZone)> {
    let mut it = period.split('-');
    let (Some(ys), Some(ms), None) = (it.next(), it.next(), it.next()) else {
        return Err(AppError::BadRequest("period must be YYYY-MM".into()));
    };
    let y: i32 = ys
        .parse()
        .map_err(|_| AppError::BadRequest("bad year in period".into()))?;
    let m: u32 = ms
        .parse()
        .map_err(|_| AppError::BadRequest("bad month in period".into()))?;
    if !(1..=12).contains(&m) {
        return Err(AppError::BadRequest("month out of range (1-12)".into()));
    }
    let mk = |yy: i32, mm: u32| {
        Utc.with_ymd_and_hms(yy, mm, 1, 0, 0, 0)
            .single()
            .ok_or_else(|| AppError::BadRequest("invalid period".into()))
    };
    let start = mk(y, m)?;
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let end = mk(ny, nm)?;
    Ok((start.fixed_offset(), end.fixed_offset()))
}

/// 由聚合原始数值装配一个对外单元格（算毛利与毛利率）。
fn make_cell(
    dim: Option<(&str, i32)>,
    revenue: Option<Decimal>,
    cost: Option<Decimal>,
    total_calls: i64,
    cost_covered_calls: i64,
) -> MarginCell {
    let revenue = revenue.unwrap_or(Decimal::ZERO);
    let cost = cost.unwrap_or(Decimal::ZERO);
    let gross_profit = revenue - cost;
    let margin_rate = (revenue > Decimal::ZERO).then(|| (gross_profit / revenue).round_dp(4));
    MarginCell {
        dim: dim.map(|(p, id)| format!("{p}:{id}")),
        dim_id: dim.map(|(_, id)| id),
        revenue,
        cost,
        gross_profit,
        margin_rate,
        total_calls,
        cost_covered_calls,
    }
}

/// `GET /api/billing/margin?period=YYYY-MM[&group_by=model|channel]`（billing.manage）
///
/// 财务毛利报表：营收 − 成本 = 毛利，按周期聚合，可按模型/渠道下钻。
pub async fn margin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<MarginQuery>,
) -> AppResult<Json<MarginResp>> {
    rise_identity::require(&state, &headers, "billing.manage").await?;
    let db = state.db()?;

    let period = match q.period {
        Some(p) => p,
        None => {
            let now = Utc::now();
            format!("{:04}-{:02}", now.year(), now.month())
        }
    };
    let (start, end) = parse_period(&period)?;

    // 共用聚合列：营收(Σcharged) / 成本(Σcost) / 总调用 / 含成本调用（COUNT(col) 只数非 NULL）。
    let base = || {
        usage_logs::Entity::find()
            .filter(usage_logs::Column::CreatedAt.gte(start))
            .filter(usage_logs::Column::CreatedAt.lt(end))
            .select_only()
            .column_as(
                Expr::col(usage_logs::Column::ChargedAmount).sum(),
                "revenue",
            )
            .column_as(Expr::col(usage_logs::Column::CostAmount).sum(), "cost")
            .column_as(Expr::col(usage_logs::Column::Id).count(), "total_calls")
            .column_as(
                Expr::col(usage_logs::Column::CostAmount).count(),
                "cost_covered_calls",
            )
    };

    let rows: Vec<MarginCell> = match q.group_by.as_deref() {
        None => {
            // 无 group_by：聚合恒返回一行（即使空周期，SUM=NULL/COUNT=0）。
            let r = base()
                .into_model::<OverviewRow>()
                .one(db)
                .await?
                .unwrap_or(OverviewRow {
                    revenue: None,
                    cost: None,
                    total_calls: 0,
                    cost_covered_calls: 0,
                });
            vec![make_cell(
                None,
                r.revenue,
                r.cost,
                r.total_calls,
                r.cost_covered_calls,
            )]
        }
        Some(gb @ ("model" | "channel")) => {
            let dim_col = if gb == "model" {
                usage_logs::Column::ModelId
            } else {
                usage_logs::Column::ChannelId
            };
            let prefix = gb; // "model" / "channel"
            base()
                .column_as(Expr::col(dim_col), "dim_id")
                .group_by(dim_col)
                .order_by_asc(dim_col)
                .into_model::<GroupRow>()
                .all(db)
                .await?
                .into_iter()
                .map(|r| {
                    make_cell(
                        Some((prefix, r.dim_id)),
                        r.revenue,
                        r.cost,
                        r.total_calls,
                        r.cost_covered_calls,
                    )
                })
                .collect()
        }
        Some(_) => {
            return Err(AppError::BadRequest(
                "group_by must be 'model' or 'channel'".into(),
            ))
        }
    };

    // 毛利完整 = 所有聚合行的成本都全覆盖（无缺成本行）。
    let cost_complete = rows.iter().all(|c| c.cost_covered_calls == c.total_calls);

    Ok(Json(MarginResp {
        period,
        group_by: q.group_by,
        cost_complete,
        rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_parses_month_to_half_open_range() {
        let (s, e) = parse_period("2026-06").unwrap();
        assert_eq!(s.to_rfc3339(), "2026-06-01T00:00:00+00:00");
        assert_eq!(e.to_rfc3339(), "2026-07-01T00:00:00+00:00");
    }

    #[test]
    fn period_december_rolls_year() {
        let (s, e) = parse_period("2026-12").unwrap();
        assert_eq!(s.to_rfc3339(), "2026-12-01T00:00:00+00:00");
        assert_eq!(e.to_rfc3339(), "2027-01-01T00:00:00+00:00");
    }

    #[test]
    fn period_rejects_bad_input() {
        assert!(parse_period("2026").is_err());
        assert!(parse_period("2026-13").is_err());
        assert!(parse_period("2026-00").is_err());
        assert!(parse_period("2026-06-01").is_err());
        assert!(parse_period("abc-06").is_err());
    }

    #[test]
    fn cell_computes_margin_and_rate() {
        // 营收 100，成本 60 → 毛利 40，毛利率 0.4
        let c = make_cell(
            Some(("model", 3)),
            Some(Decimal::from(100)),
            Some(Decimal::from(60)),
            10,
            10,
        );
        assert_eq!(c.dim.as_deref(), Some("model:3"));
        assert_eq!(c.gross_profit, Decimal::from(40));
        assert_eq!(c.margin_rate, Some("0.4".parse().unwrap()));
    }

    #[test]
    fn cell_zero_revenue_has_null_rate() {
        // 空周期：营收/成本均 NULL → 0，毛利率 null（避免除零）
        let c = make_cell(None, None, None, 0, 0);
        assert_eq!(c.revenue, Decimal::ZERO);
        assert_eq!(c.gross_profit, Decimal::ZERO);
        assert_eq!(c.margin_rate, None);
        assert!(c.dim.is_none());
    }

    #[test]
    fn cell_missing_cost_treated_as_zero_cost() {
        // 成本未配（NULL）→ 按 0 成本计，毛利率 = 1.0（偏乐观，靠 cost_complete 提示）
        let c = make_cell(Some(("channel", 5)), Some(Decimal::from(50)), None, 8, 0);
        assert_eq!(c.cost, Decimal::ZERO);
        assert_eq!(c.gross_profit, Decimal::from(50));
        assert_eq!(c.margin_rate, Some(Decimal::ONE));
    }
}
