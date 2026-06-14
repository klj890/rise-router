//! 计费金额计算纯函数：单价 × 用量 → 金额。无 DB 依赖，单测覆盖。
//!
//! 单价取自定价域解析结果（`base_unit_prices` 折前 / `final_unit_prices` percentage 折后），
//! 用量来自上游响应。全程 Decimal 保精度（财务对账），仅在落库为 numeric(18,6)。

use rust_decimal::Decimal;
use serde_json::Value;

/// token 计价的单位换算：单价以「元/百万 token」表达。
const TOKEN_UNIT_DIVISOR: i64 = 1_000_000;

/// 按计费量纲求每单位的换算除数。
/// token：单价为元/百万 token → 除以 1e6；image/second/call：单价即元/张·秒·次 → 除数 1。
fn unit_divisor(billing_unit: &str) -> Decimal {
    match billing_unit {
        "token" => Decimal::from(TOKEN_UNIT_DIVISOR),
        _ => Decimal::ONE,
    }
}

/// 把 JSON 数值叶子解析为 Decimal（经字符串，不经 f64，保绝对精度）。
fn as_decimal(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => n.to_string().parse::<Decimal>().ok(),
        _ => None,
    }
}

/// 金额 = Σ_k （quantity[k] / divisor × unit_prices[k]）。
///
/// 仅对 `quantity` 与 `unit_prices` **同时存在**的数值键累加（input/output/cache_read 等天然对齐）；
/// 任一侧缺该键则该项不计费（如上游未返回 cache 用量）。
pub fn compute_charge(billing_unit: &str, unit_prices: &Value, quantity: &Value) -> Decimal {
    let (Value::Object(prices), Value::Object(qty)) = (unit_prices, quantity) else {
        return Decimal::ZERO;
    };
    let divisor = unit_divisor(billing_unit);
    let mut total = Decimal::ZERO;
    for (k, qv) in qty {
        let (Some(q), Some(p)) = (as_decimal(qv), prices.get(k).and_then(as_decimal)) else {
            continue;
        };
        total += q / divisor * p;
    }
    // numeric(18,6) 落库精度：保留 6 位
    total.round_dp(6)
}

/// 从上游 OpenAI 兼容响应体提取 token 用量 → 标准 quantity（{input,output}）。
/// 缺 `usage` 字段（如流式分块/错误响应）返回 None，调用方据此跳过计费。
pub fn extract_token_usage(body: &Value) -> Option<Value> {
    let usage = body.get("usage")?;
    // 防御性钳制：负数用量会算出负费用（变相退款/抬高可用预算），强制下限 0
    // 以防恶意/有 bug 的上游返回负 token。
    let input = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .map(|v| v.max(0));
    let output = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .map(|v| v.max(0));
    // 两者全缺则视为无可计费用量
    if input.is_none() && output.is_none() {
        return None;
    }
    Some(serde_json::json!({
        "input": input.unwrap_or(0),
        "output": output.unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_charge_per_million() {
        // input 1.5M tokens @ 10 元/百万 = 15；output 0.5M @ 30 = 15 → 30
        let prices = json!({ "input": 10.0, "output": 30.0 });
        let qty = json!({ "input": 1_500_000, "output": 500_000 });
        assert_eq!(compute_charge("token", &prices, &qty), Decimal::from(30));
    }

    #[test]
    fn missing_price_key_skips_that_term() {
        // 上游返回了 cache 用量，但价表无 cache 单价 → 该项不计费
        let prices = json!({ "input": 10.0, "output": 30.0 });
        let qty = json!({ "input": 1_000_000, "output": 0, "cache_read": 999 });
        assert_eq!(compute_charge("token", &prices, &qty), Decimal::from(10));
    }

    #[test]
    fn fractional_precision_preserved() {
        // 1234 tokens @ 7 元/百万 = 0.008638
        let prices = json!({ "input": 7.0 });
        let qty = json!({ "input": 1234 });
        assert_eq!(
            compute_charge("token", &prices, &qty),
            "0.008638".parse::<Decimal>().unwrap()
        );
    }

    #[test]
    fn non_token_unit_no_divisor() {
        // image：2 张 @ 0.25 元/张 = 0.5
        let prices = json!({ "input": 0.25 });
        let qty = json!({ "input": 2 });
        assert_eq!(
            compute_charge("image", &prices, &qty),
            "0.5".parse::<Decimal>().unwrap()
        );
    }

    #[test]
    fn empty_or_mismatched_shapes_zero() {
        assert_eq!(
            compute_charge("token", &json!(null), &json!({})),
            Decimal::ZERO
        );
        assert_eq!(
            compute_charge("token", &json!({ "input": 10 }), &json!("nope")),
            Decimal::ZERO
        );
    }

    #[test]
    fn extract_usage_maps_openai_fields() {
        let body = json!({ "usage": { "prompt_tokens": 12, "completion_tokens": 7 } });
        assert_eq!(
            extract_token_usage(&body),
            Some(json!({ "input": 12, "output": 7 }))
        );
    }

    #[test]
    fn extract_usage_absent_is_none() {
        assert_eq!(extract_token_usage(&json!({ "id": "x" })), None);
        // usage 存在但无 token 字段 → None
        assert_eq!(extract_token_usage(&json!({ "usage": {} })), None);
    }

    #[test]
    fn extract_usage_partial_fills_zero() {
        let body = json!({ "usage": { "completion_tokens": 5 } });
        assert_eq!(
            extract_token_usage(&body),
            Some(json!({ "input": 0, "output": 5 }))
        );
    }

    #[test]
    fn extract_usage_clamps_negative_to_zero() {
        // 恶意/有 bug 的上游返回负 token → 钳到 0，避免负费用退款漏洞
        let body = json!({ "usage": { "prompt_tokens": -100, "completion_tokens": 3 } });
        assert_eq!(
            extract_token_usage(&body),
            Some(json!({ "input": 0, "output": 3 }))
        );
    }
}
