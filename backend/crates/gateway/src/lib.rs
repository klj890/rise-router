//! 网关与路由域：channels / model_channels / 路由解析（relay 转发后续切片）。
//!
//! 纯函数在 [`route`]（无 DB，单测覆盖）；[`resolve_route`] 是 DB 编排。
//! 路由与定价完全分离：仅在 `models` 处相交，互不依赖。

mod route;

pub use route::{pick_weighted, rank_routes, RouteCandidate};

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{channels, model_channels, models};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 给定模型 → 故障转移顺序的候选渠道（有效优先级/权重已算好）。
pub async fn resolve_route(
    db: &DatabaseConnection,
    model_slug: &str,
) -> AppResult<Vec<RouteCandidate>> {
    let model = models::Entity::find()
        .filter(models::Column::Slug.eq(model_slug))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mcs = model_channels::Entity::find()
        .filter(model_channels::Column::ModelId.eq(model.id))
        .filter(model_channels::Column::Enabled.eq(true))
        .all(db)
        .await?;
    if mcs.is_empty() {
        return Err(AppError::NotFound);
    }

    // 只取启用（status=1）的渠道
    let channel_ids: Vec<i32> = mcs.iter().map(|m| m.channel_id).collect();
    let ch_map: HashMap<i32, channels::Model> = channels::Entity::find()
        .filter(channels::Column::Id.is_in(channel_ids))
        .filter(channels::Column::Status.eq(1_i16))
        .all(db)
        .await?
        .into_iter()
        .map(|c| (c.id, c))
        .collect();

    let candidates: Vec<RouteCandidate> = mcs
        .into_iter()
        .filter_map(|mc| {
            ch_map.get(&mc.channel_id).map(|ch| RouteCandidate {
                channel_id: ch.id,
                channel_name: ch.name.clone(),
                protocol_adapter: ch.protocol_adapter.clone(),
                base_url: ch.base_url.clone(),
                upstream_model_name: mc.upstream_model_name,
                priority: mc.priority.unwrap_or(ch.priority),
                weight: mc.weight.unwrap_or(ch.weight),
            })
        })
        .collect();
    if candidates.is_empty() {
        return Err(AppError::NotFound);
    }
    Ok(rank_routes(candidates))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "gateway ok" }))
        .route("/route", get(route_preview))
}

#[derive(Deserialize)]
struct RouteQuery {
    model: String,
}

#[derive(Serialize)]
struct RouteResponse {
    model: String,
    candidates: Vec<RouteCandidate>,
}

/// `GET /api/gateway/route?model=gpt-4o` —— 路由预览：候选渠道按故障转移顺序返回。
async fn route_preview(
    State(state): State<AppState>,
    Query(q): Query<RouteQuery>,
) -> AppResult<Json<RouteResponse>> {
    let db = state.db()?;
    let candidates = resolve_route(db, &q.model).await?;
    Ok(Json(RouteResponse {
        model: q.model,
        candidates,
    }))
}
