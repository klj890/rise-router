//! 价格表（定价五要素·显式单价；模型×分组；版本化）管理 CRUD。
//!
//! 价格只挂在 (model, group) 维度，显式单价 jsonb（元/百万 token 等直观单位，**无倍率**）。
//! 实体无密钥、已派生 serde，响应直接返回 [`prices::Model`]。
//!
//! 关键语义：
//! - `billing_unit` **从模型派生**（须与 model 一致），不接受客户端传入，去掉错配 footgun；
//! - **版本化**：创建时按 (model, group, billing_unit) 自动 `version = max+1`；`select_price`
//!   同档取最新 version → 新版自然取代旧版，旧版仍留作历史（计费/审计）。改价 = 建新版本，
//!   不联动渠道/分组/折扣（五要素解耦）；
//! - `unit_prices` 校验为**非空扁平**数值映射（每值 ≥0；不允许嵌套——折扣按比例缩放会污染非价字段，分档定价待专门结构）；
//! - 价格是定价线叶子（无下游引用），删除直接删——管理员自负"删光价 → 该档无价"之责。

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{groups, models, prices};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::Deserialize;
use serde_json::Value;

/// currency 清洗：trim + 大写；空 → 默认 CNY；长度 3..=8 且全字母。
fn clean_currency(c: Option<String>) -> AppResult<String> {
    let c = c.unwrap_or_default();
    let c = c.trim().to_uppercase();
    if c.is_empty() {
        return Ok("CNY".to_owned());
    }
    if !(3..=8).contains(&c.chars().count()) || !c.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err(AppError::BadRequest(
            "currency must be a 3-8 letter code (e.g. CNY, USD)".into(),
        ));
    }
    Ok(c)
}

/// unit_prices 须为**非空扁平对象**，每个值都是有限且 ≥0 的数值（价格）。
///
/// 刻意不允许嵌套对象/数组：折扣按比例缩放 unit_prices 的每个数值（见 `resolve::scale_numeric`），
/// 嵌套结构里的非价字段（如分档的 up_to/数量）会被一并缩放而算错。分档/分辨率定价待**专门的
/// 分档结构 + 分档感知的缩放**落地后再开放（见 docs §11）。
fn validate_unit_prices(v: &Value) -> AppResult<()> {
    let Value::Object(o) = v else {
        return Err(AppError::BadRequest(
            "unit_prices must be a non-empty object".into(),
        ));
    };
    if o.is_empty() {
        return Err(AppError::BadRequest(
            "unit_prices must be a non-empty object".into(),
        ));
    }
    for (k, val) in o {
        let Value::Number(n) = val else {
            return Err(AppError::BadRequest(format!(
                "unit_prices.{k} must be a number (nested/array pricing not supported yet)"
            )));
        };
        let f = n.as_f64().unwrap_or(-1.0);
        if !f.is_finite() || f < 0.0 {
            return Err(AppError::BadRequest(format!(
                "unit_prices.{k} must be finite and >= 0"
            )));
        }
    }
    Ok(())
}

/// valid_to 若存在须严格晚于 valid_from。
fn validate_window(from: DateTimeWithTimeZone, to: Option<DateTimeWithTimeZone>) -> AppResult<()> {
    if let Some(to) = to {
        if to <= from {
            return Err(AppError::BadRequest(
                "valid_to must be after valid_from".into(),
            ));
        }
    }
    Ok(())
}

/// 同 (model, group, billing_unit) 档下一个 version（max+1，空档从 1 起）。
async fn next_version(
    db: &DatabaseConnection,
    model_id: i32,
    group_id: Option<i32>,
    billing_unit: &str,
) -> AppResult<i32> {
    let mut q = prices::Entity::find()
        .filter(prices::Column::ModelId.eq(model_id))
        .filter(prices::Column::BillingUnit.eq(billing_unit));
    q = match group_id {
        Some(g) => q.filter(prices::Column::GroupId.eq(g)),
        None => q.filter(prices::Column::GroupId.is_null()),
    };
    let top = q.order_by_desc(prices::Column::Version).one(db).await?;
    Ok(top.map(|p| p.version + 1).unwrap_or(1))
}

#[derive(Deserialize)]
pub struct CreateReq {
    model_id: i32,
    /// 为空 = 该模型默认价；非空 = 该分组专属价
    group_id: Option<i32>,
    currency: Option<String>,
    /// 显式单价 jsonb，如 `{"input":1.5,"output":6.0}`（元/百万 token）
    unit_prices: Value,
    /// 缺省 = 此刻生效
    valid_from: Option<DateTimeWithTimeZone>,
    /// 缺省 = 开口（长期有效）
    valid_to: Option<DateTimeWithTimeZone>,
}

/// `POST /api/pricing/prices`（admin）—— 新建价格（自动定版本号）。billing_unit 派生自模型。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<prices::Model>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;

    // 模型必须存在；billing_unit 取模型快照（不接受客户端传入，杜绝错配）。
    let model = models::Entity::find_by_id(req.model_id)
        .one(db)
        .await?
        .ok_or(AppError::BadRequest("model_id not found".into()))?;
    // 分组（若指定）必须存在。
    if let Some(g) = req.group_id {
        if groups::Entity::find_by_id(g).one(db).await?.is_none() {
            return Err(AppError::BadRequest("group_id not found".into()));
        }
    }

    let currency = clean_currency(req.currency)?;
    validate_unit_prices(&req.unit_prices)?;
    let valid_from = req
        .valid_from
        .unwrap_or_else(|| chrono::Utc::now().fixed_offset());
    validate_window(valid_from, req.valid_to)?;

    let version = next_version(db, req.model_id, req.group_id, &model.billing_unit).await?;

    let m = prices::ActiveModel {
        model_id: Set(req.model_id),
        group_id: Set(req.group_id),
        billing_unit: Set(model.billing_unit),
        currency: Set(currency),
        unit_prices: Set(req.unit_prices),
        valid_from: Set(valid_from),
        valid_to: Set(req.valid_to),
        version: Set(version),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct ListQuery {
    model_id: Option<i32>,
    /// 注意：仅在显式传时按分组过滤；不传 = 不过滤（含默认价与各分组价）
    group_id: Option<i32>,
}

/// `GET /api/pricing/prices`（admin）—— 列出价格，可按 model_id/group_id 过滤，id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<prices::Model>>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;
    let mut query = prices::Entity::find();
    if let Some(mid) = q.model_id {
        query = query.filter(prices::Column::ModelId.eq(mid));
    }
    if let Some(gid) = q.group_id {
        query = query.filter(prices::Column::GroupId.eq(gid));
    }
    let rows = query.order_by_asc(prices::Column::Id).all(db).await?;
    Ok(Json(rows))
}

/// `GET /api/pricing/prices/{id}`（admin）—— 取单条价格。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<prices::Model>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;
    let m = prices::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    currency: Option<String>,
    unit_prices: Option<Value>,
    valid_from: Option<DateTimeWithTimeZone>,
    /// Some=设到期；None=不变（撤销到期请建新版本）
    valid_to: Option<DateTimeWithTimeZone>,
}

/// `PUT /api/pricing/prices/{id}`（admin）—— 部分更新（model/group/billing_unit/version 不可改：价格身份）。
/// 常规改价请「建新版本」让旧版自然被取代；本端点用于改单价/币种/有效期等就地修正。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<prices::Model>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;

    let existing = prices::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    // 合并后校验有效期窗口（from 取更新值否则原值；to 同理但 None=不变 → 取原值参与校验）。
    let eff_from = req.valid_from.unwrap_or(existing.valid_from);
    let eff_to = req.valid_to.or(existing.valid_to);
    validate_window(eff_from, eff_to)?;

    let mut am: prices::ActiveModel = existing.into();
    if let Some(c) = req.currency {
        am.currency = Set(clean_currency(Some(c))?);
    }
    if let Some(up) = req.unit_prices {
        validate_unit_prices(&up)?;
        am.unit_prices = Set(up);
    }
    if let Some(f) = req.valid_from {
        am.valid_from = Set(f);
    }
    if let Some(t) = req.valid_to {
        am.valid_to = Set(Some(t));
    }

    let m = am.update(db).await?;
    Ok(Json(m))
}

/// `DELETE /api/pricing/prices/{id}`（admin）—— 删除价格（定价线叶子，直接删）。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;
    let res = prices::Entity::delete_by_id(id).exec(db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn currency_defaults_normalizes_and_validates() {
        assert_eq!(clean_currency(None).unwrap(), "CNY");
        assert_eq!(clean_currency(Some("  ".into())).unwrap(), "CNY");
        assert_eq!(clean_currency(Some("usd".into())).unwrap(), "USD");
        assert!(clean_currency(Some("U$".into())).is_err());
        assert!(clean_currency(Some("TOOLONGCUR".into())).is_err());
    }

    #[test]
    fn unit_prices_requires_flat_nonneg_numbers() {
        assert!(validate_unit_prices(&json!({"input": 1.5, "output": 6.0})).is_ok());
        assert!(validate_unit_prices(&json!({"per_image": 0.2})).is_ok());
        assert!(validate_unit_prices(&json!({})).is_err()); // 空
        assert!(validate_unit_prices(&json!([1, 2])).is_err()); // 非对象
        assert!(validate_unit_prices(&json!({"input": -1.0})).is_err()); // 负价
        assert!(validate_unit_prices(&json!({"label": "x"})).is_err()); // 非数值
                                                                        // 嵌套/数组不再允许（折扣缩放会污染非价字段）：
        assert!(validate_unit_prices(&json!({"hd": {"price": 0.2}})).is_err());
        assert!(validate_unit_prices(&json!({"tiers": [0.1, 0.2]})).is_err());
    }

    #[test]
    fn window_rejects_non_increasing() {
        use chrono::{TimeZone, Utc};
        let a = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .unwrap()
            .fixed_offset();
        let b = Utc
            .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
            .unwrap()
            .fixed_offset();
        assert!(validate_window(a, None).is_ok());
        assert!(validate_window(a, Some(b)).is_ok());
        assert!(validate_window(b, Some(a)).is_err());
        assert!(validate_window(a, Some(a)).is_err());
    }
}
