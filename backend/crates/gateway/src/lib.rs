//! 网关与路由域：channels / model_channels / 路由解析（relay 转发后续切片）。
//!
//! 纯函数在 [`route`]（无 DB，单测覆盖）；[`resolve_route`] 是 DB 编排。
//! 路由与定价完全分离：仅在 `models` 处相交，互不依赖。

mod adapter;
mod channel;
pub mod health;
mod model;
mod model_channel;
mod relay;
mod route;

pub use relay::relay_routes;
pub use route::{pick_weighted, rank_routes, weighted_failover_order, RouteCandidate};

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{channels, model_channels, models};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

/// 已实现的协议族白名单。**新厂商若属已知协议族 = 纯配置接入**（建渠道选此值即可）；
/// 全新协议族须先写适配器代码（`adapter/`）再加入此列表。渠道 CRUD 据此拒绝自由文本拼错。
/// 必须与 [`adapter::adapter_for`] 的分支保持一致（白名单放行的协议族必须有适配器）。
// chat 协议族（gateway relay 路由）+ 任务协议族（rise-task 的 TaskAdapter 路由，渠道经此 CRUD 创建）。
pub const KNOWN_PROTOCOL_ADAPTERS: &[&str] =
    &["openai_compatible", "anthropic", "gemini", "mock_task"];

/// 给定模型 → 故障转移顺序的候选渠道（有效优先级/权重已算好）。
pub async fn resolve_route(
    db: &DatabaseConnection,
    model_slug: &str,
) -> AppResult<Vec<RouteCandidate>> {
    let model = models::find_listed_by_slug(db, model_slug)
        .await?
        .ok_or(AppError::NotFound)?;

    // 单次 LEFT JOIN 取启用路由 + 其渠道（避免 N+1）；渠道熔断/禁用在内存过滤。
    let rows = model_channels::Entity::find()
        .filter(model_channels::Column::ModelId.eq(model.id))
        .filter(model_channels::Column::Enabled.eq(true))
        .find_also_related(channels::Entity)
        .all(db)
        .await?;

    let candidates: Vec<RouteCandidate> = rows
        .into_iter()
        .filter_map(|(mc, ch)| {
            let ch = ch?;
            if ch.status != channels::ChannelStatus::Enabled {
                return None;
            }
            Some(RouteCandidate {
                channel_id: ch.id,
                channel_name: ch.name,
                protocol_adapter: ch.protocol_adapter,
                base_url: ch.base_url,
                upstream_model_name: mc.upstream_model_name,
                priority: mc.priority.unwrap_or(ch.priority),
                weight: mc.weight.unwrap_or(ch.weight),
            })
        })
        .collect();
    if candidates.is_empty() {
        // 模型存在（上架）但无健康渠道（全部禁用/熔断）→ 503，区别于"模型不存在"的 404
        return Err(AppError::Unavailable);
    }
    Ok(rank_routes(candidates))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "gateway ok" }))
        .route("/route", get(route_preview))
        // 渠道管理 CRUD（admin 守卫，凭据脱敏）
        .route("/channels", post(channel::create).get(channel::list))
        .route(
            "/channels/{id}",
            get(channel::get_one)
                .put(channel::update)
                .delete(channel::delete),
        )
        // 渠道连通性测试（真打上游，五层判定，写回测速）
        .route("/channels/{id}/test", post(channel::test))
        // 模型目录管理 CRUD（admin 守卫）
        .route("/models", post(model::create).get(model::list))
        .route(
            "/models/{id}",
            get(model::get_one).put(model::update).delete(model::delete),
        )
        // 路由线（model↔channel）管理 CRUD（admin 守卫）
        .route(
            "/model-channels",
            post(model_channel::create).get(model_channel::list),
        )
        .route(
            "/model-channels/{id}",
            get(model_channel::get_one)
                .put(model_channel::update)
                .delete(model_channel::delete),
        )
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
