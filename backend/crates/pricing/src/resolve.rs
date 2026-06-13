//! 定价解析纯函数：价格选取 + 折扣应用。无 DB 依赖，便于单测。
//!
//! 设计原则（docs/data-model.md §5）：最终价 = 查表得确定单价 + 显式折扣叠加，
//! 规则可见；改任一要素不联动其余四要素。

use rise_entity::{discounts, prices};
use rust_decimal::prelude::ToPrimitive;
use sea_orm::prelude::{DateTimeWithTimeZone, Decimal};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct AppliedDiscount {
    pub id: i32,
    pub name: String,
    /// percentage / fixed
    pub kind: String,
    pub value: f64,
    /// percentage：true 表示已并入 final_unit_prices；fixed：恒 false（结算期作用于账单总额）
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedPrice {
    pub model_slug: String,
    pub group_slug: Option<String>,
    pub billing_unit: String,
    pub currency: String,
    pub base_unit_prices: Value,
    /// 已应用 percentage 折扣后的单价
    pub final_unit_prices: Value,
    /// 累计 percentage 折扣系数（1.0 = 无折扣）
    pub discount_factor: f64,
    pub applied_discounts: Vec<AppliedDiscount>,
    pub price_version: i32,
}

fn is_valid_at(
    from: DateTimeWithTimeZone,
    to: Option<DateTimeWithTimeZone>,
    at: DateTimeWithTimeZone,
) -> bool {
    from <= at && to.is_none_or(|t| t > at)
}

/// 选出最匹配的有效价格：分组专属价优先于默认价（group_id NULL），同档取最新 version。
/// 一次查表即得"该分组该模型的确定单价"。
pub fn select_price(
    prices: &[prices::Model],
    group_id: Option<i32>,
    at: DateTimeWithTimeZone,
) -> Option<&prices::Model> {
    prices
        .iter()
        .filter(|p| is_valid_at(p.valid_from, p.valid_to, at))
        .filter(|p| p.group_id.is_none() || p.group_id == group_id)
        .max_by(|a, b| {
            // 命中请求分组的专属价排在默认价（None）之前；同档比 version
            let rank = |p: &prices::Model| (group_id.is_some() && p.group_id == group_id) as i32;
            rank(a).cmp(&rank(b)).then(a.version.cmp(&b.version))
        })
}

fn targets(d: &discounts::Model, model_id: i32, group_id: Option<i32>) -> bool {
    match d.scope.as_str() {
        "global" => true,
        "model" => d.target_model_id == Some(model_id),
        "group" => group_id.is_some() && d.target_group_id == group_id,
        "model_group" => {
            d.target_model_id == Some(model_id)
                && group_id.is_some()
                && d.target_group_id == group_id
        }
        // org 维度折扣需 organizations 上下文，不在单价解析阶段处理
        _ => false,
    }
}

fn dec_to_f64(d: Decimal) -> f64 {
    d.to_f64().unwrap_or(1.0)
}

fn pct(d: &discounts::Model, applied: bool) -> AppliedDiscount {
    AppliedDiscount {
        id: d.id,
        name: d.name.clone(),
        kind: "percentage".into(),
        value: dec_to_f64(d.value),
        applied,
    }
}

/// 应用折扣：percentage 并入单价，fixed 仅登记（结算期作用于账单总额）。
/// 叠加规则显式：存在不可叠加折扣时取优先级最高的单独一条；否则全部可叠加者相乘。
pub fn apply_discounts(
    price: &prices::Model,
    discounts: &[discounts::Model],
    model_id: i32,
    group_id: Option<i32>,
    at: DateTimeWithTimeZone,
) -> (Value, f64, Vec<AppliedDiscount>) {
    let applicable: Vec<&discounts::Model> = discounts
        .iter()
        .filter(|d| is_valid_at(d.valid_from, d.valid_to, at))
        .filter(|d| targets(d, model_id, group_id))
        .collect();

    let percentage: Vec<&discounts::Model> = applicable
        .iter()
        .copied()
        .filter(|d| d.kind == "percentage")
        .collect();

    // 折扣系数全程用 Decimal 计算，仅在 API 边界转 f64，避免浮点累积误差（财务对账）。
    let mut factor = Decimal::ONE;
    let mut applied = Vec::new();

    match percentage
        .iter()
        .filter(|d| !d.stackable)
        .max_by_key(|d| d.priority)
    {
        Some(ns) => {
            // 不可叠加：仅用优先级最高的这一条
            factor = ns.value;
            for d in &percentage {
                applied.push(pct(d, d.id == ns.id));
            }
        }
        None => {
            // 全部可叠加：相乘
            for d in &percentage {
                factor *= d.value;
                applied.push(pct(d, true));
            }
        }
    }

    for d in applicable.iter().filter(|d| d.kind == "fixed") {
        applied.push(AppliedDiscount {
            id: d.id,
            name: d.name.clone(),
            kind: "fixed".into(),
            value: dec_to_f64(d.value),
            applied: false,
        });
    }

    (
        scale_numeric(&price.unit_prices, factor),
        dec_to_f64(factor),
        applied,
    )
}

/// 递归把 JSON 数值叶子乘以 factor（按比例折扣单价），全程 Decimal 保精度，保留 6 位小数。
/// 注：假设数值叶子均为价格（token 类 {input,output,cache_read} 成立）；分档结构中若有
/// 非价格数值字段（如 up_to/min_tokens），后续按 billing_unit 结构精化（当前 resolution 等为字符串，安全）。
fn scale_numeric(v: &Value, factor: Decimal) -> Value {
    match v {
        Value::Number(n) => {
            // 直接由 JSON 数字的字符串形式解析为 Decimal，全程不经 f64，保绝对精度
            let val = n.to_string().parse::<Decimal>().unwrap_or(Decimal::ZERO);
            (val * factor)
                .round_dp(6)
                .to_string()
                .parse::<serde_json::Number>()
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| scale_numeric(x, factor)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), scale_numeric(x, factor)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn at(y: i32, mo: u32, d: u32) -> DateTimeWithTimeZone {
        Utc.with_ymd_and_hms(y, mo, d, 0, 0, 0)
            .unwrap()
            .fixed_offset()
    }

    fn price(id: i32, group_id: Option<i32>, version: i32, up: Value) -> prices::Model {
        prices::Model {
            id,
            model_id: 1,
            group_id,
            billing_unit: "token".into(),
            currency: "CNY".into(),
            unit_prices: up,
            valid_from: at(2026, 1, 1),
            valid_to: None,
            version,
        }
    }

    fn disc(
        id: i32,
        scope: &str,
        kind: &str,
        value: &str,
        stackable: bool,
        priority: i32,
    ) -> discounts::Model {
        discounts::Model {
            id,
            name: format!("d{id}"),
            scope: scope.into(),
            target_org_id: None,
            target_group_id: Some(10),
            target_model_id: Some(1),
            kind: kind.into(),
            value: value.parse().unwrap(),
            stackable,
            priority,
            valid_from: at(2026, 1, 1),
            valid_to: None,
        }
    }

    #[test]
    fn group_specific_price_beats_default() {
        let prices = vec![
            price(1, None, 1, json!({ "input": 10.0 })),
            price(2, Some(10), 1, json!({ "input": 7.0 })),
        ];
        let p = select_price(&prices, Some(10), at(2026, 6, 1)).unwrap();
        assert_eq!(p.id, 2);
    }

    #[test]
    fn falls_back_to_default_when_no_group_price() {
        let prices = vec![price(1, None, 1, json!({ "input": 10.0 }))];
        let p = select_price(&prices, Some(99), at(2026, 6, 1)).unwrap();
        assert_eq!(p.id, 1);
    }

    #[test]
    fn highest_version_wins_same_tier() {
        let prices = vec![
            price(1, Some(10), 1, json!({ "input": 10.0 })),
            price(2, Some(10), 3, json!({ "input": 8.0 })),
            price(3, Some(10), 2, json!({ "input": 9.0 })),
        ];
        let p = select_price(&prices, Some(10), at(2026, 6, 1)).unwrap();
        assert_eq!(p.version, 3);
    }

    #[test]
    fn expired_price_excluded() {
        let mut expired = price(1, Some(10), 1, json!({ "input": 7.0 }));
        expired.valid_to = Some(at(2026, 3, 1));
        let prices = vec![expired, price(2, None, 1, json!({ "input": 10.0 }))];
        let p = select_price(&prices, Some(10), at(2026, 6, 1)).unwrap();
        assert_eq!(p.id, 2); // 专属价过期 → 回落默认价
    }

    #[test]
    fn stackable_percentage_discounts_multiply() {
        let p = price(1, Some(10), 1, json!({ "input": 100.0, "output": 200.0 }));
        let ds = vec![
            disc(1, "group", "percentage", "0.9", true, 0),
            disc(2, "model", "percentage", "0.8", true, 0),
        ];
        let (final_p, factor, applied) = apply_discounts(&p, &ds, 1, Some(10), at(2026, 6, 1));
        assert!((factor - 0.72).abs() < 1e-9); // 0.9 * 0.8
        assert_eq!(final_p["input"], json!(72.0));
        assert_eq!(final_p["output"], json!(144.0));
        assert!(applied.iter().all(|a| a.applied));
    }

    #[test]
    fn non_stackable_picks_highest_priority_only() {
        let p = price(1, Some(10), 1, json!({ "input": 100.0 }));
        let ds = vec![
            disc(1, "global", "percentage", "0.5", false, 10),
            disc(2, "group", "percentage", "0.9", false, 1),
        ];
        let (final_p, factor, applied) = apply_discounts(&p, &ds, 1, Some(10), at(2026, 6, 1));
        assert!((factor - 0.5).abs() < 1e-9); // 仅优先级 10 的那条
        assert_eq!(final_p["input"], json!(50.0));
        assert_eq!(applied.iter().filter(|a| a.applied).count(), 1);
    }

    #[test]
    fn fixed_discount_registered_but_not_applied_to_unit_price() {
        let p = price(1, Some(10), 1, json!({ "input": 100.0 }));
        let ds = vec![disc(1, "global", "fixed", "20", false, 0)];
        let (final_p, factor, applied) = apply_discounts(&p, &ds, 1, Some(10), at(2026, 6, 1));
        assert_eq!(factor, 1.0);
        assert_eq!(final_p["input"], json!(100.0)); // 单价不变
        assert_eq!(applied.len(), 1);
        assert!(!applied[0].applied);
    }

    #[test]
    fn discount_scope_filtering() {
        let p = price(1, Some(10), 1, json!({ "input": 100.0 }));
        // model 维度但 target_model_id 不匹配
        let mut wrong = disc(1, "model", "percentage", "0.5", true, 0);
        wrong.target_model_id = Some(999);
        let (_, factor, _) = apply_discounts(&p, &[wrong], 1, Some(10), at(2026, 6, 1));
        assert_eq!(factor, 1.0); // 不适用
    }
}
