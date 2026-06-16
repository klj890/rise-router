//! 月度毛利月报 cron（M2 片F·Part2）。
//!
//! 每分钟 tick：到/过本月预定时间「day 号 hour 点」（CST/UTC+8）且本月未发 → 取**上月**毛利
//! （[`compute_margin`]）→ HTML 正文 + xlsx 附件（[`render_margin`]）→ SMTP 发送
//! → 写 `cron_state` 防重（进程重启不重发）。
//!
//! org 无邮箱字段，本报是「平台全量毛利月报」发给固定财务收件人；per-org 客户账单留 M3。
//! **自愈式触发**：判定用「now ≥ 本月预定时间」而非精确小时匹配——窗口内宕机 / DB / SMTP
//! 故障后，当月任意时刻重启都会补发；cron_state 防重保证只发一次。时间一律按 CST 判断。

use axum::{extract::State, http::HeaderMap, Json};
use chrono::{DateTime, Datelike, FixedOffset, TimeZone, Utc};
use rise_core::{AppError, AppResult, AppState};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use serde::Serialize;
use std::time::Duration;

use crate::email::{send_report, XlsxAttachment};
use crate::export::render_margin;
use crate::margin::{compute_margin, MarginResp};

const LAST_SENT_KEY: &str = "billing.monthly.last_sent";

fn cst() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("CST(+08:00) 偏移合法")
}

/// 启动月报 cron（仅在 `enabled` 时进入循环；否则记一行日志返回）。
pub fn spawn(state: AppState) {
    if !state.config.billing_email.enabled {
        tracing::info!("billing email cron disabled (RR_BILLING_EMAIL_ENABLED != true)");
        return;
    }
    tokio::spawn(async move {
        // 启动延迟，避开迁移/seed 启动期竞争。
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            if let Err(e) = tick(&state).await {
                tracing::warn!("billing email cron tick err: {e}");
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn tick(state: &AppState) -> AppResult<()> {
    let cfg = &state.config.billing_email;
    let now = Utc::now().with_timezone(&cst());
    let db = state.db()?;
    let last = get_state(db, LAST_SENT_KEY)
        .await?
        .and_then(|s| s.parse::<i64>().ok());
    if !should_send(now, cfg.day, cfg.hour, last) {
        return Ok(());
    }
    tracing::info!("billing email cron: firing monthly report (now >= scheduled, not yet sent)");
    let outcome = run_send(state).await?;
    set_state(db, LAST_SENT_KEY, &now.timestamp().to_string()).await?;
    tracing::info!(
        "billing email cron sent: period={} recipients={} dry_run={}",
        outcome.period,
        outcome.recipients,
        outcome.dry_run
    );
    Ok(())
}

/// 是否应发本月月报：到/过本月预定时间（day 号 hour 点，CST）且本月未发。
///
/// 自愈式——窗口内宕机 / DB / SMTP 故障后，当月任意时刻重启都会补发；`cron_state` 防重
/// （[`sent_this_month`]）保证只发一次。`day`/`hour` 已在 config 规范化到合法范围。
fn should_send(now: DateTime<FixedOffset>, day: u32, hour: u32, last_sent: Option<i64>) -> bool {
    let Some(scheduled) = cst()
        .with_ymd_and_hms(now.year(), now.month(), day, hour, 0, 0)
        .single()
    else {
        return false; // 理论不达（config 已规范化 day≤28 / hour≤23）
    };
    now >= scheduled && !sent_this_month(last_sent, now.timestamp())
}

/// 一次月报发送的结果（test 端点回显 / cron 记日志用）。
#[derive(Serialize)]
pub(crate) struct SendOutcome {
    pub period: String,
    pub recipients: usize,
    pub dry_run: bool,
    pub xlsx_bytes: usize,
}

/// 组装并发送上月月报。cron 与 test 端点共用。
pub(crate) async fn run_send(state: &AppState) -> AppResult<SendOutcome> {
    let cfg = &state.config.billing_email;
    let db = state.db()?;

    let now = Utc::now().with_timezone(&cst());
    let period = prev_period(now.year(), now.month());
    let resp = compute_margin(db, Some(period.clone()), Some("model".into())).await?;

    let subject = format!("[Rise Router 月报] {period} 平台毛利");
    let html = build_html(&resp);
    let xlsx = render_margin(&resp).map_err(|e| AppError::Internal(format!("xlsx 渲染: {e}")))?;
    let outcome = SendOutcome {
        period: period.clone(),
        recipients: cfg.recipients.len(),
        dry_run: cfg.dry_run,
        xlsx_bytes: xlsx.len(),
    };

    // dry-run 不需要 SMTP：本地/无 SMTP 也能验证渲染 + 组装整条链路。
    if cfg.dry_run {
        tracing::info!(
            "billing email DRY-RUN: subject={subject:?} recipients={:?} html_len={} xlsx_bytes={}",
            cfg.recipients,
            html.len(),
            xlsx.len()
        );
        return Ok(outcome);
    }

    // 真发才需要 SMTP 配置。
    let smtp =
        state.config.smtp.as_ref().ok_or_else(|| {
            AppError::BadRequest("SMTP 未配置（RR_SMTP_HOST / RR_SMTP_FROM）".into())
        })?;
    let attachment = XlsxAttachment {
        filename: format!("margin-{period}.xlsx"),
        bytes: xlsx,
    };
    send_report(smtp, &cfg.recipients, &subject, html, Some(attachment)).await?;
    Ok(outcome)
}

/// `POST /api/billing/email/test`（billing.manage）—— 手动触发一次月报（便于不等月初验证）。
/// dry-run 下不真发，仅回显组装结果；用于本地/无 SMTP 验证整条链路。
pub async fn email_test(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<SendOutcome>> {
    rise_identity::require(&state, &headers, "billing.manage").await?;
    Ok(Json(run_send(&state).await?))
}

/// 上一个自然月（YYYY-MM）。1 月回退到上一年 12 月。
fn prev_period(year: i32, month: u32) -> String {
    let (y, m) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    format!("{y:04}-{m:02}")
}

/// `last_sent` 是否与 `now` 落在同一自然月（CST）→ 本月已发，防重。
fn sent_this_month(last_ts: Option<i64>, now_ts: i64) -> bool {
    let to_ym = |ts: i64| {
        cst()
            .timestamp_opt(ts, 0)
            .single()
            .map(|d| (d.year(), d.month()))
    };
    match (last_ts.and_then(to_ym), to_ym(now_ts)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// 毛利月报 HTML 正文：总览（rows 汇总）+ 按模型明细表。
fn build_html(resp: &MarginResp) -> String {
    let money = |d: Decimal| format!("{:.2}", d.round_dp(2));
    let (mut rev, mut cost, mut gp) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    for c in &resp.rows {
        rev += c.revenue;
        cost += c.cost;
        gp += c.gross_profit;
    }
    let rate = if rev > Decimal::ZERO {
        format!(
            "{:.2}%",
            (gp / rev * Decimal::from(100)).to_f64().unwrap_or(0.0)
        )
    } else {
        "—".to_string()
    };
    let cost_note = if resp.cost_complete {
        String::new()
    } else {
        "<p style=\"color:#c0392b\">注：部分调用缺渠道成本，毛利偏乐观（未配成本按 0 计）。</p>"
            .to_string()
    };

    let mut model_rows = String::new();
    for c in &resp.rows {
        let dim = c.dim.as_deref().unwrap_or("总览");
        let mr = c
            .margin_rate
            .map(|r| format!("{:.2}%", (r * Decimal::from(100)).to_f64().unwrap_or(0.0)))
            .unwrap_or_else(|| "—".to_string());
        model_rows.push_str(&format!(
            "<tr><td>{}</td><td align=\"right\">{}</td><td align=\"right\">{}</td>\
             <td align=\"right\">{}</td><td align=\"right\">{}</td><td align=\"right\">{}</td></tr>",
            dim,
            money(c.revenue),
            money(c.cost),
            money(c.gross_profit),
            mr,
            c.total_calls
        ));
    }

    format!(
        "<div style=\"font-family:sans-serif\">\
         <h2>Rise Router 平台毛利月报 · {period}</h2>\
         <p>统计口径：营收（Σ 实扣）− 成本（Σ 渠道成本）= 毛利。</p>\
         {cost_note}\
         <table border=\"1\" cellpadding=\"6\" cellspacing=\"0\" style=\"border-collapse:collapse\">\
         <tr><td>营收</td><td align=\"right\">{rev}</td></tr>\
         <tr><td>成本</td><td align=\"right\">{cost}</td></tr>\
         <tr><td>毛利</td><td align=\"right\">{gp}</td></tr>\
         <tr><td>毛利率</td><td align=\"right\">{rate}</td></tr>\
         </table>\
         <h3>按模型</h3>\
         <table border=\"1\" cellpadding=\"6\" cellspacing=\"0\" style=\"border-collapse:collapse\">\
         <tr><th>模型</th><th>营收</th><th>成本</th><th>毛利</th><th>毛利率</th><th>调用数</th></tr>\
         {model_rows}\
         </table>\
         <p>完整明细见附件 xlsx。</p>\
         </div>",
        period = resp.period,
        rev = money(rev),
        cost = money(cost),
        gp = money(gp),
    )
}

/// 读 cron_state[key]。
async fn get_state(db: &DatabaseConnection, key: &str) -> AppResult<Option<String>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            "SELECT value FROM cron_state WHERE key = $1",
            [key.into()],
        ))
        .await?;
    match row {
        Some(r) => Ok(Some(r.try_get("", "value")?)),
        None => Ok(None),
    }
}

/// UPSERT cron_state[key] = value（带 updated_at）。
async fn set_state(db: &DatabaseConnection, key: &str, value: &str) -> AppResult<()> {
    let backend = db.get_database_backend();
    let now = Utc::now().fixed_offset();
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        "INSERT INTO cron_state (key, value, updated_at) VALUES ($1, $2, $3) \
         ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = $3",
        [key.into(), value.into(), now.into()],
    ))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::margin::MarginCell;

    #[test]
    fn prev_period_rolls_back_month_and_year() {
        assert_eq!(prev_period(2026, 6), "2026-05");
        assert_eq!(prev_period(2026, 1), "2025-12"); // 1 月回退到上年 12 月
        assert_eq!(prev_period(2026, 12), "2026-11");
    }

    #[test]
    fn sent_this_month_detects_same_cst_month() {
        // 2026-06-15 12:00 CST = 1781? 用构造时间戳。
        let ts = cst()
            .with_ymd_and_hms(2026, 6, 15, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let same_month = cst()
            .with_ymd_and_hms(2026, 6, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let next_month = cst()
            .with_ymd_and_hms(2026, 7, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert!(sent_this_month(Some(ts), same_month)); // 同月 → 已发
        assert!(!sent_this_month(Some(ts), next_month)); // 跨月 → 可发
        assert!(!sent_this_month(None, same_month)); // 从未发 → 可发
    }

    #[test]
    fn should_send_self_heals_and_dedups() {
        let at = |m, d, h| {
            cst()
                .with_ymd_and_hms(2026, m, d, h, 0, 0)
                .single()
                .unwrap()
        };
        let ts = |dt: DateTime<FixedOffset>| dt.timestamp();
        // 未到本月预定时间（6-01 09:00 之前）→ 不发
        assert!(!should_send(at(6, 1, 8), 1, 9, None));
        // 到点且本月未发 → 发
        assert!(should_send(at(6, 1, 9), 1, 9, None));
        // 月中开机、本月未发（预定窗口内曾宕机）→ 补发（自愈）
        assert!(should_send(at(6, 15, 3), 1, 9, None));
        // 本月已发 → 跳过（防重），即便月中多次 tick
        assert!(!should_send(at(6, 15, 3), 1, 9, Some(ts(at(6, 1, 9)))));
        // 上月发过、本月到点 → 发（跨月）
        assert!(should_send(at(6, 1, 9), 1, 9, Some(ts(at(5, 1, 9)))));
    }

    #[test]
    fn build_html_contains_totals_and_rows() {
        let resp = MarginResp {
            period: "2026-05".into(),
            group_by: Some("model".into()),
            cost_complete: false,
            rows: vec![
                MarginCell {
                    dim: Some("model:3".into()),
                    dim_id: Some(3),
                    revenue: Decimal::from(100),
                    cost: Decimal::from(60),
                    gross_profit: Decimal::from(40),
                    margin_rate: Some("0.4".parse().unwrap()),
                    total_calls: 10,
                    cost_covered_calls: 10,
                },
                MarginCell {
                    dim: Some("model:5".into()),
                    dim_id: Some(5),
                    revenue: Decimal::from(50),
                    cost: Decimal::ZERO,
                    gross_profit: Decimal::from(50),
                    margin_rate: None,
                    total_calls: 8,
                    cost_covered_calls: 0,
                },
            ],
        };
        let html = build_html(&resp);
        assert!(html.contains("2026-05"));
        assert!(html.contains("model:3") && html.contains("model:5"));
        assert!(html.contains("150.00")); // 营收合计 100+50
        assert!(html.contains("90.00")); // 毛利合计 40+50
        assert!(html.contains("偏乐观")); // cost_complete=false 提示
    }
}
