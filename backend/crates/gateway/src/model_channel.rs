//! 路由线（model↔channel 能力可达 + 负载）管理 CRUD。
//!
//! 路由表只管「能力可达 + 负载」，剥离 group/售价（售价在 prices，二者分离）。
//! 实体无密钥、已派生 serde，响应直接返回 [`model_channels::Model`]。
//! 创建前校验 model_id/channel_id 均存在（否则 FK 触发 500）且 (model,channel) 未重复
//! （唯一约束 `uq_model_channels`，预检防 500）。
//! priority/weight 为 NULL 时继承渠道默认值；**更新走部分语义**（未提供=不变），
//! 要将 priority/weight/cost_price 重置回「继承」请删路由重建（避免 absent/null 二义的过度设计）。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{admin_guard, AppError, AppResult, AppState};
use rise_entity::{channels, model_channels, models};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::Deserialize;
use serde_json::Value;

fn validate_upstream_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest(
            "upstream_model_name is required".into(),
        ));
    }
    if name.chars().count() > 128 {
        return Err(AppError::BadRequest(
            "upstream_model_name too long (max 128)".into(),
        ));
    }
    Ok(name.to_owned())
}

/// priority/weight 若提供须 ≥ 0（负值在加权随机/优先级分层里无意义）。
fn validate_nonneg(field: &str, v: Option<i32>) -> AppResult<()> {
    if let Some(v) = v {
        if v < 0 {
            return Err(AppError::BadRequest(format!("{field} must be >= 0")));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateReq {
    model_id: i32,
    channel_id: i32,
    upstream_model_name: String,
    /// 缺省 = true（新路由默认接流量）
    enabled: Option<bool>,
    /// NULL/缺省 = 继承渠道 priority
    priority: Option<i32>,
    /// NULL/缺省 = 继承渠道 weight
    weight: Option<i32>,
    /// 渠道成本价 jsonb（按 billing_unit）；与售价 prices 分离
    cost_price: Option<Value>,
}

/// `POST /api/gateway/model-channels`（admin）—— 新建路由线。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<model_channels::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let upstream = validate_upstream_name(&req.upstream_model_name)?;
    validate_nonneg("priority", req.priority)?;
    validate_nonneg("weight", req.weight)?;

    // FK 预检：model_id/channel_id 必须存在（否则 insert 触发 FK 失败 → 500）。
    if models::Entity::find_by_id(req.model_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::BadRequest("model_id not found".into()));
    }
    if channels::Entity::find_by_id(req.channel_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::BadRequest("channel_id not found".into()));
    }
    // 唯一 (model, channel) 预检（uq_model_channels）：重复 → 400，而非 DB 唯一约束 500。
    let dup = model_channels::Entity::find()
        .filter(model_channels::Column::ModelId.eq(req.model_id))
        .filter(model_channels::Column::ChannelId.eq(req.channel_id))
        .count(db)
        .await?;
    if dup > 0 {
        return Err(AppError::BadRequest(
            "route for this (model, channel) already exists".into(),
        ));
    }

    let m = model_channels::ActiveModel {
        model_id: Set(req.model_id),
        channel_id: Set(req.channel_id),
        upstream_model_name: Set(upstream),
        enabled: Set(req.enabled.unwrap_or(true)),
        priority: Set(req.priority),
        weight: Set(req.weight),
        cost_price: Set(req.cost_price),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// 可选按模型过滤（看某模型挂了哪些渠道）
    model_id: Option<i32>,
    /// 可选按渠道过滤
    channel_id: Option<i32>,
}

/// `GET /api/gateway/model-channels`（admin）—— 列出路由线，可按 model_id/channel_id 过滤，id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> AppResult<Json<Vec<model_channels::Model>>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let mut query = model_channels::Entity::find();
    if let Some(mid) = q.model_id {
        query = query.filter(model_channels::Column::ModelId.eq(mid));
    }
    if let Some(cid) = q.channel_id {
        query = query.filter(model_channels::Column::ChannelId.eq(cid));
    }
    let rows = query
        .order_by_asc(model_channels::Column::Id)
        .all(db)
        .await?;
    Ok(Json(rows))
}

/// `GET /api/gateway/model-channels/{id}`（admin）—— 取单条路由线。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<model_channels::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let m = model_channels::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    upstream_model_name: Option<String>,
    enabled: Option<bool>,
    /// Some=设值（须 ≥0）；None=不变（重置回继承请删路由重建）
    priority: Option<i32>,
    weight: Option<i32>,
    /// Some=替换成本价；None=不变
    cost_price: Option<Value>,
}

/// `PUT /api/gateway/model-channels/{id}`（admin）—— 部分更新（model_id/channel_id 不可改：路由身份）。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<model_channels::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;

    let existing = model_channels::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: model_channels::ActiveModel = existing.into();

    if let Some(name) = req.upstream_model_name {
        am.upstream_model_name = Set(validate_upstream_name(&name)?);
    }
    if let Some(enabled) = req.enabled {
        am.enabled = Set(enabled);
    }
    if let Some(p) = req.priority {
        validate_nonneg("priority", Some(p))?;
        am.priority = Set(Some(p));
    }
    if let Some(w) = req.weight {
        validate_nonneg("weight", Some(w))?;
        am.weight = Set(Some(w));
    }
    if let Some(cost) = req.cost_price {
        am.cost_price = Set(Some(cost));
    }

    let m = am.update(db).await?;
    Ok(Json(m))
}

/// `DELETE /api/gateway/model-channels/{id}`（admin）—— 删除路由线。无下游引用，直接删。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let res = model_channels::Entity::delete_by_id(id).exec(db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_name_trims_and_bounds() {
        assert_eq!(
            validate_upstream_name("  gpt-4o-2024 ").unwrap(),
            "gpt-4o-2024"
        );
        assert!(validate_upstream_name("  ").is_err());
        assert!(validate_upstream_name(&"x".repeat(129)).is_err());
    }

    #[test]
    fn nonneg_allows_none_and_rejects_negative() {
        assert!(validate_nonneg("weight", None).is_ok());
        assert!(validate_nonneg("weight", Some(0)).is_ok());
        assert!(validate_nonneg("priority", Some(3)).is_ok());
        assert!(validate_nonneg("priority", Some(-1)).is_err());
    }
}
