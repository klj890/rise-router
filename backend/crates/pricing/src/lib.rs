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
use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;

/// 取数后调纯函数解析最终价。
pub async fn resolve_price(
    db: &DatabaseConnection,
    model_slug: &str,
    group_slug: Option<&str>,
    at: DateTimeWithTimeZone,
) -> AppResult<ResolvedPrice> {
    let model = models::find_listed_by_slug(db, model_slug)
        .await?
        .ok_or(AppError::NotFound)?;

    // 显式指定但查无此分组 → 报错，不静默回落默认价（避免拼错 slug 却默默按默认价计费）。
    let group = match group_slug {
        Some(s) => Some(
            groups::Entity::find()
                .filter(groups::Column::Slug.eq(s))
                .one(db)
                .await?
                .ok_or(AppError::NotFound)?,
        ),
        None => None,
    };
    let group_id = group.as_ref().map(|g| g.id);

    // 在 SQL 中过滤（命中 idx_prices_lookup）：默认价 + 该分组专属价，且当前有效。
    let group_price_cond = match group_id {
        Some(g) => Condition::any()
            .add(prices::Column::GroupId.is_null())
            .add(prices::Column::GroupId.eq(g)),
        None => Condition::all().add(prices::Column::GroupId.is_null()),
    };
    let model_prices = prices::Entity::find()
        .filter(prices::Column::ModelId.eq(model.id))
        .filter(group_price_cond)
        .filter(prices::Column::ValidFrom.lte(at))
        .filter(
            Condition::any()
                .add(prices::Column::ValidTo.is_null())
                .add(prices::Column::ValidTo.gt(at)),
        )
        .all(db)
        .await?;
    let selected = select_price(&model_prices, group_id, at).ok_or(AppError::NotFound)?;

    // 折扣同样在 SQL 中按有效期 + 适用 scope 过滤，避免热路径全表扫描。
    let mut scope_cond = Condition::any()
        .add(discounts::Column::Scope.eq("global"))
        .add(
            Condition::all()
                .add(discounts::Column::Scope.eq("model"))
                .add(discounts::Column::TargetModelId.eq(model.id)),
        );
    if let Some(g) = group_id {
        scope_cond = scope_cond
            .add(
                Condition::all()
                    .add(discounts::Column::Scope.eq("group"))
                    .add(discounts::Column::TargetGroupId.eq(g)),
            )
            .add(
                Condition::all()
                    .add(discounts::Column::Scope.eq("model_group"))
                    .add(discounts::Column::TargetModelId.eq(model.id))
                    .add(discounts::Column::TargetGroupId.eq(g)),
            );
    }
    let all_discounts = discounts::Entity::find()
        .filter(discounts::Column::ValidFrom.lte(at))
        .filter(
            Condition::any()
                .add(discounts::Column::ValidTo.is_null())
                .add(discounts::Column::ValidTo.gt(at)),
        )
        .filter(scope_cond)
        .all(db)
        .await?;
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
