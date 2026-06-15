//! 财务报表 xlsx 导出（M2 片F·Part1）。
//!
//! 复用对账（reconcile 落库的对账单）与毛利（[`compute_margin`]）的查询，渲染为 xlsx 下载——
//! 「所见即所导」：导出与页面/热路径共用同一取数函数，不另写一套聚合。
//! 渲染是纯函数（数据 → 字节），无 DB 即可单测；handler 仅做鉴权 + 取数 + 包装响应。
//! 文件名仅含 ASCII（report 名 + period），故 Content-Disposition 用简单 quoted-string，
//! 无需 RFC 5987 编码与 urlencoding 依赖。

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::reconciliations;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, XlsxError};
use sea_orm::EntityTrait;
use serde::Deserialize;

use crate::margin::{compute_margin, MarginResp};

/// 表头底色：与前端控制台主色一致（CITIC 深蓝）。
const HEADER_BG: u32 = 0x1A3A6E;

fn header_fmt() -> Format {
    Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(HEADER_BG))
        .set_align(FormatAlign::Center)
}

/// 货币格式：四位小数千分位（金额量纲与 prices 一致，保留细粒度）。
fn money_fmt() -> Format {
    Format::new().set_num_format("#,##0.0000")
}

/// 把 xlsx 字节作为附件响应。文件名为纯 ASCII，简单 quoted-string 即可。
fn xlsx_response(bytes: Vec<u8>, filename: &str) -> Response {
    let mut resp = (StatusCode::OK, bytes).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    );
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    resp
}

/// 对账单 detail（jsonb）的模型级明细行。与 reconcile 写入的形状对齐；
/// rust_decimal 的 Deserialize 同时兼容 JSON string 与 number，故 revenue 用 Decimal 直收。
#[derive(Deserialize, Default)]
struct DetailLine {
    model_id: i32,
    revenue: Decimal,
    calls: i64,
}

/// 渲染对账单 xlsx：Sheet1 汇总 + Sheet2 模型明细。
fn render_reconciliation(m: &reconciliations::Model) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let hf = header_fmt();
    let mf = money_fmt();

    // Sheet 1：对账汇总（key/value 两列）。
    {
        let s = wb.add_worksheet().set_name("对账汇总")?;
        let status = match m.status {
            reconciliations::ReconStatus::Draft => "草稿",
            reconciliations::ReconStatus::Locked => "已封账",
        };
        let dash = || "—".to_string();
        let kv: [(&str, String); 8] = [
            ("周期", m.period.clone()),
            ("状态", status.to_string()),
            ("应收营收", m.total_revenue.to_string()),
            ("调用数", m.total_calls.to_string()),
            (
                "上游成本",
                m.upstream_cost.map(|d| d.to_string()).unwrap_or_else(dash),
            ),
            (
                "毛利缺口",
                m.gap.map(|d| d.to_string()).unwrap_or_else(dash),
            ),
            ("生成时间", m.generated_at.to_rfc3339()),
            (
                "封账时间",
                m.locked_at.map(|d| d.to_rfc3339()).unwrap_or_else(dash),
            ),
        ];
        for (i, (k, v)) in kv.iter().enumerate() {
            s.write_string_with_format(i as u32, 0, *k, &hf)?;
            s.write_string(i as u32, 1, v)?;
        }
        s.set_column_width(0, 14.0)?;
        s.set_column_width(1, 30.0)?;
    }

    // Sheet 2：模型级明细（从 detail jsonb 解析；缺失/解析失败 → 空表）。
    {
        let s = wb.add_worksheet().set_name("模型明细")?;
        for (c, h) in ["模型 ID", "营收", "调用数"].iter().enumerate() {
            s.write_string_with_format(0, c as u16, *h, &hf)?;
        }
        let lines: Vec<DetailLine> = m
            .detail
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        for (i, line) in lines.iter().enumerate() {
            let r = (i + 1) as u32;
            s.write_number(r, 0, line.model_id as f64)?;
            s.write_number_with_format(r, 1, line.revenue.to_f64().unwrap_or(0.0), &mf)?;
            s.write_number(r, 2, line.calls as f64)?;
        }
        s.set_column_width(0, 12.0)?;
        s.set_column_width(1, 16.0)?;
        s.set_column_width(2, 12.0)?;
    }

    wb.save_to_buffer()
}

/// 渲染毛利报表 xlsx：元信息（周期/维度/成本完整）+ 明细行。
fn render_margin(resp: &MarginResp) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let s = wb.add_worksheet().set_name("毛利报表")?;
    let hf = header_fmt();
    let mf = money_fmt();

    // 元信息区。
    s.write_string(0, 0, "周期")?;
    s.write_string(0, 1, &resp.period)?;
    s.write_string(1, 0, "下钻维度")?;
    s.write_string(1, 1, resp.group_by.as_deref().unwrap_or("总览"))?;
    s.write_string(2, 0, "成本完整")?;
    s.write_string(
        2,
        1,
        if resp.cost_complete {
            "是"
        } else {
            "否（部分行缺成本，毛利偏乐观）"
        },
    )?;

    // 明细表（第 5 行起，留一行空白）。
    let hrow = 4u32;
    for (c, h) in [
        "维度",
        "营收",
        "成本",
        "毛利",
        "毛利率",
        "调用数",
        "含成本调用",
    ]
    .iter()
    .enumerate()
    {
        s.write_string_with_format(hrow, c as u16, *h, &hf)?;
    }
    for (i, cell) in resp.rows.iter().enumerate() {
        let r = hrow + 1 + i as u32;
        s.write_string(r, 0, cell.dim.as_deref().unwrap_or("总览"))?;
        s.write_number_with_format(r, 1, cell.revenue.to_f64().unwrap_or(0.0), &mf)?;
        s.write_number_with_format(r, 2, cell.cost.to_f64().unwrap_or(0.0), &mf)?;
        s.write_number_with_format(r, 3, cell.gross_profit.to_f64().unwrap_or(0.0), &mf)?;
        match cell.margin_rate {
            Some(rate) => s.write_number(r, 4, rate.to_f64().unwrap_or(0.0))?,
            None => s.write_string(r, 4, "—")?,
        };
        s.write_number(r, 5, cell.total_calls as f64)?;
        s.write_number(r, 6, cell.cost_covered_calls as f64)?;
    }
    for c in 0..=6u16 {
        s.set_column_width(c, 16.0)?;
    }

    wb.save_to_buffer()
}

/// `GET /api/billing/reconciliations/{id}/export`（billing.manage）—— 对账单 xlsx 下载。
pub async fn export_reconciliation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Response> {
    rise_identity::require(&state, &headers, "billing.manage").await?;
    let db = state.db()?;

    let m = reconciliations::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let bytes =
        render_reconciliation(&m).map_err(|e| AppError::Internal(format!("xlsx render: {e}")))?;
    Ok(xlsx_response(
        bytes,
        &format!("reconciliation-{}.xlsx", m.period),
    ))
}

#[derive(Deserialize)]
pub struct MarginExportQuery {
    period: Option<String>,
    group_by: Option<String>,
}

/// `GET /api/billing/margin/export?period=YYYY-MM[&group_by=model|channel]`（billing.manage）
/// —— 毛利报表 xlsx 下载。与 `/margin`（JSON）共用 [`compute_margin`]。
pub async fn export_margin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<MarginExportQuery>,
) -> AppResult<Response> {
    rise_identity::require(&state, &headers, "billing.manage").await?;
    let db = state.db()?;

    let resp = compute_margin(db, q.period, q.group_by).await?;
    let bytes =
        render_margin(&resp).map_err(|e| AppError::Internal(format!("xlsx render: {e}")))?;
    Ok(xlsx_response(
        bytes,
        &format!("margin-{}.xlsx", resp.period),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::margin::MarginCell;

    /// xlsx 是 zip 容器，magic number 为 `PK`（0x50 0x4B）。
    fn assert_xlsx(bytes: &[u8]) {
        assert!(bytes.len() > 100, "xlsx 字节过短: {}", bytes.len());
        assert_eq!(&bytes[..2], b"PK", "应为 zip(xlsx) magic");
    }

    fn sample_margin() -> MarginResp {
        MarginResp {
            period: "2026-06".into(),
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
        }
    }

    #[test]
    fn margin_xlsx_renders_zip() {
        assert_xlsx(&render_margin(&sample_margin()).unwrap());
    }

    #[test]
    fn reconciliation_xlsx_renders_zip_with_detail() {
        let m = reconciliations::Model {
            id: 1,
            period: "2026-06".into(),
            status: reconciliations::ReconStatus::Locked,
            total_revenue: Decimal::from(1000),
            total_calls: 50,
            upstream_cost: None,
            gap: None,
            // revenue 用 string 形式，校验 DetailLine 从 rust_decimal string 反序列化无损。
            detail: Some(serde_json::json!([
                {"model_id": 3, "revenue": "600", "calls": 30},
                {"model_id": 5, "revenue": "400", "calls": 20}
            ])),
            generated_at: chrono::Utc::now().fixed_offset(),
            locked_at: Some(chrono::Utc::now().fixed_offset()),
        };
        assert_xlsx(&render_reconciliation(&m).unwrap());
    }

    #[test]
    fn reconciliation_xlsx_handles_missing_detail() {
        let m = reconciliations::Model {
            id: 2,
            period: "2026-07".into(),
            status: reconciliations::ReconStatus::Draft,
            total_revenue: Decimal::ZERO,
            total_calls: 0,
            upstream_cost: None,
            gap: None,
            detail: None,
            generated_at: chrono::Utc::now().fixed_offset(),
            locked_at: None,
        };
        assert_xlsx(&render_reconciliation(&m).unwrap());
    }
}
