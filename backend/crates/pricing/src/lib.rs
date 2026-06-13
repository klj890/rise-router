//! 定价域：价格表 / 折扣 / resolve_price 解析（查表 + 折扣叠加）。
//!
//! 纯函数在 [`resolve`]（无 DB，单测覆盖）；[`resolve_price`] 是 DB 编排。
//! 管理台「价格预览」与网关计费热路径复用同一解析，保证所见即所得。

mod resolve;

pub use resolve::{apply_discounts, select_price, AppliedDiscount, ResolvedPrice};

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{discounts, groups, models, prices};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;

/// 取数后调纯函数解析最终价。
pub async fn resolve_price(
    db: &DatabaseConnection,
    model_slug: &str,
    group_slug: Option<&str>,
    at: DateTimeWithTimeZone,
) -> AppResult<ResolvedPrice> {
    let model = models::Entity::find()
        .filter(models::Column::Slug.eq(model_slug))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    let group = match group_slug {
        Some(s) => {
            groups::Entity::find()
                .filter(groups::Column::Slug.eq(s))
                .one(db)
                .await?
        }
        None => None,
    };
    let group_id = group.as_ref().map(|g| g.id);

    let model_prices = prices::Entity::find()
        .filter(prices::Column::ModelId.eq(model.id))
        .all(db)
        .await?;
    let selected = select_price(&model_prices, group_id, at).ok_or(AppError::NotFound)?;

    let all_discounts = discounts::Entity::find().all(db).await?;
    let (final_unit_prices, discount_factor, applied_discounts) =
        apply_discounts(selected, &all_discounts, model.id, group_id, at);

    Ok(ResolvedPrice {
        model_slug: model.slug,
        group_slug: group.map(|g| g.slug),
        billing_unit: selected.billing_unit.clone(),
        currency: selected.currency.clone(),
        base_unit_prices: selected.unit_prices.clone(),
        final_unit_prices,
        discount_factor,
        applied_discounts,
        price_version: selected.version,
    })
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "pricing ok" }))
        .route("/preview", get(preview))
}

#[derive(Deserialize)]
struct PreviewQuery {
    model: String,
    group: Option<String>,
}

/// `GET /api/pricing/preview?model=gpt-4o&group=vip` —— 价格预览（所见即所得）。
async fn preview(
    State(state): State<AppState>,
    Query(q): Query<PreviewQuery>,
) -> AppResult<Json<ResolvedPrice>> {
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();
    let resolved = resolve_price(db, &q.model, q.group.as_deref(), now).await?;
    Ok(Json(resolved))
}
