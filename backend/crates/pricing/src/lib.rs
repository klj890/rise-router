//! 定价域：价格表 / 折扣 / resolve_price 解析（查表 + 折扣叠加）。
//!
//! 纯函数在 [`resolve`]（无 DB，单测覆盖）；[`resolve_price`] 是 DB 编排。
//! 管理台「价格预览」与网关计费热路径复用同一解析，保证所见即所得。

mod discount;
mod group;
mod price;
mod resolve;

pub use resolve::{apply_discounts, select_price, AppliedDiscount, ResolvedPrice};

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{discounts, groups, models, prices};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;

/// 按分组 **slug** 解析最终价（管理台预览：用户输入 slug）。
/// 显式指定但查无此分组 → 报错，不静默回落默认价（避免拼错 slug 却默默按默认价计费）。
pub async fn resolve_price(
    db: &DatabaseConnection,
    model_slug: &str,
    group_slug: Option<&str>,
    at: DateTimeWithTimeZone,
) -> AppResult<ResolvedPrice> {
    let model = load_model(db, model_slug).await?;
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
    resolve_for(db, &model, group.as_ref(), at).await
}

/// 按分组 **id** 解析最终价（网关计费热路径：KeyContext 已带 group_id，免去 slug→id 二次查）。
/// 与 [`resolve_price`] 复用同一 core 解析，保证「所见即所得」。
pub async fn resolve_price_by_group_id(
    db: &DatabaseConnection,
    model_slug: &str,
    group_id: Option<i32>,
    at: DateTimeWithTimeZone,
) -> AppResult<ResolvedPrice> {
    let model = load_model(db, model_slug).await?;
    // 计费热路径容错：分组被删/数据不一致时回落默认价（group=None），绝不因此中断结算。
    // 与 slug 版的"严格报错"刻意不同——slug 来自人工输入需防拼错；group_id 来自 org 记录，
    // 删组只应降级到默认价，而非让该次调用因 NotFound 结算失败被 at-least-serve 免单（丢收入）。
    let group = match group_id {
        Some(g) => groups::Entity::find_by_id(g).one(db).await?,
        None => None,
    };
    resolve_for(db, &model, group.as_ref(), at).await
}

/// 计价不限模型状态：下架模型仍需解析历史价格（计费/审计）；新流量由网关路由拦截。
async fn load_model(db: &DatabaseConnection, model_slug: &str) -> AppResult<models::Model> {
    models::find_by_slug(db, model_slug)
        .await?
        .ok_or(AppError::NotFound)
}

/// 价格 + 折扣解析的 core（接已加载的 model/group，slug 版与 id 版共用）。
async fn resolve_for(
    db: &DatabaseConnection,
    model: &models::Model,
    group: Option<&groups::Model>,
    at: DateTimeWithTimeZone,
) -> AppResult<ResolvedPrice> {
    let group_id = group.map(|g| g.id);

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
        model_id: model.id,
        model_slug: model.slug.clone(),
        group_slug: group.map(|g| g.slug.clone()),
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
        // 商业分组管理 CRUD（admin 守卫）
        .route("/groups", post(group::create).get(group::list))
        .route(
            "/groups/{id}",
            get(group::get_one).put(group::update).delete(group::delete),
        )
        // 价格表管理 CRUD（admin 守卫，自动定版本号）
        .route("/prices", post(price::create).get(price::list))
        .route(
            "/prices/{id}",
            get(price::get_one).put(price::update).delete(price::delete),
        )
        // 折扣管理 CRUD（admin 守卫）
        .route("/discounts", post(discount::create).get(discount::list))
        .route(
            "/discounts/{id}",
            get(discount::get_one)
                .put(discount::update)
                .delete(discount::delete),
        )
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
