//! 折扣（定价五要素·独立显式实体；可叠加规则可见）管理 CRUD。
//!
//! 折扣是与价格解耦的独立实体：按 scope（global/model/group/model_group/org）命中，
//! kind=percentage（乘数因子并入单价）/ fixed（结算期作用账单总额）。叠加规则见
//! [`super::apply_discounts`]。实体无密钥、已派生 serde，响应直接返回 [`discounts::Model`]。
//!
//! 校验：scope/kind 白名单；percentage 因子 ∈ (0,1]（如 0.9=九折），fixed > 0；
//! 按 scope 预检目标实体存在（target_* 无 FK，悬空会静默不命中，故预检防配错）。
//! 更新走部分语义，**scope/kind/targets 不可改**（改这些请删后重建，避免 scope→target 重校验泥潭）。

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{admin_guard, AppError, AppResult, AppState};
use rise_entity::{discounts, groups, models, organizations};
use rust_decimal::Decimal;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::Deserialize;

const KNOWN_SCOPES: &[&str] = &["global", "model", "group", "model_group", "org"];
const KNOWN_KINDS: &[&str] = &["percentage", "fixed"];

fn validate_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if name.chars().count() > 128 {
        return Err(AppError::BadRequest("name too long (max 128)".into()));
    }
    Ok(name.to_owned())
}

fn validate_in(field: &str, value: &str, allowed: &[&str]) -> AppResult<String> {
    let v = value.trim();
    if !allowed.contains(&v) {
        return Err(AppError::BadRequest(format!(
            "unknown {field} (allowed: {})",
            allowed.join(", ")
        )));
    }
    Ok(v.to_owned())
}

/// 按 kind 校验 value（round 到 decimal(16,4)）：percentage∈(0,1]，fixed>0。
fn validate_value(kind: &str, value: Decimal) -> AppResult<Decimal> {
    let v = value.round_dp(4);
    match kind {
        "percentage" => {
            if v <= Decimal::ZERO || v > Decimal::ONE {
                return Err(AppError::BadRequest(
                    "percentage value must be in (0, 1] (e.g. 0.9 for 10% off)".into(),
                ));
            }
        }
        "fixed" => {
            if v <= Decimal::ZERO {
                return Err(AppError::BadRequest("fixed value must be > 0".into()));
            }
        }
        _ => return Err(AppError::BadRequest("invalid kind".into())),
    }
    Ok(v)
}

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

/// 按 scope 解析并预检目标实体（无 FK，悬空会静默不命中）。返回 (org, group, model)。
async fn resolve_targets(
    db: &DatabaseConnection,
    scope: &str,
    org: Option<i32>,
    group: Option<i32>,
    model: Option<i32>,
) -> AppResult<(Option<i32>, Option<i32>, Option<i32>)> {
    async fn ensure_model(db: &DatabaseConnection, id: i32) -> AppResult<()> {
        if models::Entity::find_by_id(id).one(db).await?.is_none() {
            return Err(AppError::BadRequest("target_model_id not found".into()));
        }
        Ok(())
    }
    async fn ensure_group(db: &DatabaseConnection, id: i32) -> AppResult<()> {
        if groups::Entity::find_by_id(id).one(db).await?.is_none() {
            return Err(AppError::BadRequest("target_group_id not found".into()));
        }
        Ok(())
    }
    async fn ensure_org(db: &DatabaseConnection, id: i32) -> AppResult<()> {
        if organizations::Entity::find_by_id(id)
            .one(db)
            .await?
            .is_none()
        {
            return Err(AppError::BadRequest("target_org_id not found".into()));
        }
        Ok(())
    }

    match scope {
        "global" => Ok((None, None, None)),
        "model" => {
            let m = model.ok_or(AppError::BadRequest(
                "target_model_id required for scope=model".into(),
            ))?;
            ensure_model(db, m).await?;
            Ok((None, None, Some(m)))
        }
        "group" => {
            let g = group.ok_or(AppError::BadRequest(
                "target_group_id required for scope=group".into(),
            ))?;
            ensure_group(db, g).await?;
            Ok((None, Some(g), None))
        }
        "model_group" => {
            let m = model.ok_or(AppError::BadRequest(
                "target_model_id required for scope=model_group".into(),
            ))?;
            let g = group.ok_or(AppError::BadRequest(
                "target_group_id required for scope=model_group".into(),
            ))?;
            ensure_model(db, m).await?;
            ensure_group(db, g).await?;
            Ok((None, Some(g), Some(m)))
        }
        "org" => {
            let o = org.ok_or(AppError::BadRequest(
                "target_org_id required for scope=org".into(),
            ))?;
            ensure_org(db, o).await?;
            Ok((Some(o), None, None))
        }
        _ => Err(AppError::BadRequest("invalid scope".into())),
    }
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    scope: String,
    kind: String,
    value: Decimal,
    target_org_id: Option<i32>,
    target_group_id: Option<i32>,
    target_model_id: Option<i32>,
    stackable: Option<bool>,
    priority: Option<i32>,
    valid_from: Option<DateTimeWithTimeZone>,
    valid_to: Option<DateTimeWithTimeZone>,
}

/// `POST /api/pricing/discounts`（admin）—— 新建折扣。按 scope 预检目标存在性。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<discounts::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let name = validate_name(&req.name)?;
    let scope = validate_in("scope", &req.scope, KNOWN_SCOPES)?;
    let kind = validate_in("kind", &req.kind, KNOWN_KINDS)?;
    let value = validate_value(&kind, req.value)?;
    let (org, group, model) = resolve_targets(
        db,
        &scope,
        req.target_org_id,
        req.target_group_id,
        req.target_model_id,
    )
    .await?;
    let valid_from = req
        .valid_from
        .unwrap_or_else(|| chrono::Utc::now().fixed_offset());
    validate_window(valid_from, req.valid_to)?;

    let m = discounts::ActiveModel {
        name: Set(name),
        scope: Set(scope),
        kind: Set(kind),
        value: Set(value),
        target_org_id: Set(org),
        target_group_id: Set(group),
        target_model_id: Set(model),
        stackable: Set(req.stackable.unwrap_or(false)),
        priority: Set(req.priority.unwrap_or(0)),
        valid_from: Set(valid_from),
        valid_to: Set(req.valid_to),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct ListQuery {
    scope: Option<String>,
}

/// `GET /api/pricing/discounts`（admin）—— 列出折扣，可按 scope 过滤，id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<discounts::Model>>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let mut query = discounts::Entity::find();
    if let Some(scope) = q.scope {
        query = query.filter(discounts::Column::Scope.eq(scope));
    }
    let rows = query.order_by_asc(discounts::Column::Id).all(db).await?;
    Ok(Json(rows))
}

/// `GET /api/pricing/discounts/{id}`（admin）—— 取单条折扣。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<discounts::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let m = discounts::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    /// 按现有 kind 校验
    value: Option<Decimal>,
    stackable: Option<bool>,
    priority: Option<i32>,
    valid_from: Option<DateTimeWithTimeZone>,
    valid_to: Option<DateTimeWithTimeZone>,
}

/// `PUT /api/pricing/discounts/{id}`（admin）—— 部分更新（scope/kind/targets 不可改）。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<discounts::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let existing = discounts::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    let eff_from = req.valid_from.unwrap_or(existing.valid_from);
    let eff_to = req.valid_to.or(existing.valid_to);
    validate_window(eff_from, eff_to)?;

    let kind = existing.kind.clone();
    let mut am: discounts::ActiveModel = existing.into();
    if let Some(name) = req.name {
        am.name = Set(validate_name(&name)?);
    }
    if let Some(value) = req.value {
        am.value = Set(validate_value(&kind, value)?);
    }
    if let Some(s) = req.stackable {
        am.stackable = Set(s);
    }
    if let Some(p) = req.priority {
        am.priority = Set(p);
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

/// `DELETE /api/pricing/discounts/{id}`（admin）—— 删除折扣（独立实体，无下游引用）。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let res = discounts::Entity::delete_by_id(id).exec(db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn value_bounds_by_kind() {
        assert_eq!(
            validate_value("percentage", dec("0.9")).unwrap(),
            dec("0.9")
        );
        assert_eq!(validate_value("percentage", dec("1")).unwrap(), dec("1"));
        assert!(validate_value("percentage", dec("0")).is_err());
        assert!(validate_value("percentage", dec("1.5")).is_err());
        assert_eq!(validate_value("fixed", dec("20")).unwrap(), dec("20"));
        assert!(validate_value("fixed", dec("0")).is_err());
        assert!(validate_value("fixed", dec("-5")).is_err());
    }

    #[test]
    fn scope_and_kind_allowlists() {
        assert!(validate_in("scope", "model_group", KNOWN_SCOPES).is_ok());
        assert!(validate_in("scope", "tenant", KNOWN_SCOPES).is_err());
        assert!(validate_in("kind", "percentage", KNOWN_KINDS).is_ok());
        assert!(validate_in("kind", "coupon", KNOWN_KINDS).is_err());
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
        assert!(validate_window(a, Some(b)).is_ok());
        assert!(validate_window(b, Some(a)).is_err());
    }
}
