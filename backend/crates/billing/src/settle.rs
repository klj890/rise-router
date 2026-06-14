//! 同步后扣结算：上游成功响应 → resolve_price → 算费 → 写流水 + 自增预算。
//!
//! 「同步」：在请求路径内、返回客户端前完成 usage_logs 落库与预算自增。
//! 「后扣」：按实际 usage 扣减，允许跨越上限的那一次调用成功，随后翻 Exhausted 拒绝后续
//! （MVP 接受瞬时透支，预扣/冻结留待真遇并发透支痛点再做——见 docs/roadmap）。
//!
//! 结算失败由调用方（relay）吞掉并 log，**不影响已成功的上游响应**（at-least-serve）。

use rise_core::AppResult;
use rise_entity::{api_keys, usage_logs};
use rise_pricing::resolve_price_by_group_id;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::Value;

use crate::charge::compute_charge;

/// 一次 chat 调用的结算输入（字段较多，聚为结构体避免长参数列表）。
pub struct ChatSettlement<'a> {
    pub org_id: i32,
    pub user_id: Option<i32>,
    pub api_key_id: i32,
    pub app_id: Option<i32>,
    /// 组织商业分组（喂 resolve_price；None = 默认价）
    pub group_id: Option<i32>,
    pub model_slug: &'a str,
    pub channel_id: i32,
    /// 用量 {input,output}
    pub quantity: Value,
    pub latency_ms: Option<i32>,
    pub request_id: Option<String>,
    pub is_stream: bool,
}

/// 结算一次 chat 调用：解析最终价 → 算折前/折后金额 → 插流水 → 原子自增预算 → 必要时翻 Exhausted。
pub async fn settle_chat(
    db: &DatabaseConnection,
    s: ChatSettlement<'_>,
    at: DateTimeWithTimeZone,
) -> AppResult<()> {
    // 与网关路由/管理台预览复用同一解析（所见即所得）。group_id 已在手，免 slug→id 二次查。
    let resolved = resolve_price_by_group_id(db, s.model_slug, s.group_id, at).await?;

    // base = 折前；charged = percentage 折后（fixed 折扣留对账期，已记入 discount_detail）。
    let base = compute_charge(
        &resolved.billing_unit,
        &resolved.base_unit_prices,
        &s.quantity,
    );
    let charged = compute_charge(
        &resolved.billing_unit,
        &resolved.final_unit_prices,
        &s.quantity,
    );
    let discount_detail = serde_json::to_value(&resolved.applied_discounts).ok();

    // 1. 追加流水
    usage_logs::ActiveModel {
        org_id: Set(s.org_id),
        user_id: Set(s.user_id),
        api_key_id: Set(s.api_key_id),
        app_id: Set(s.app_id),
        model_id: Set(resolved.model_id),
        channel_id: Set(s.channel_id),
        group_slug: Set(resolved.group_slug.clone()),
        request_id: Set(s.request_id.clone()),
        billing_unit: Set(resolved.billing_unit.clone()),
        quantity: Set(s.quantity.clone()),
        base_amount: Set(base),
        discount_detail: Set(discount_detail),
        charged_amount: Set(charged),
        // 渠道成本字段未建 → 毛利留后续
        cost_amount: Set(None),
        latency_ms: Set(s.latency_ms),
        is_stream: Set(s.is_stream),
        created_at: Set(at),
        ..Default::default()
    }
    .insert(db)
    .await?;

    // 2. 原子自增预算（col_expr 走 SQL `budget_used = budget_used + charged`，避免读改写竞态）
    api_keys::Entity::update_many()
        .col_expr(
            api_keys::Column::BudgetUsed,
            Expr::col(api_keys::Column::BudgetUsed).add(charged),
        )
        .filter(api_keys::Column::Id.eq(s.api_key_id))
        .exec(db)
        .await?;

    // 3. 自增后若已达上限 → 翻 Exhausted（后续调用在鉴权阶段即被拒，返回 429）
    api_keys::Entity::update_many()
        .col_expr(
            api_keys::Column::Status,
            Expr::value(api_keys::KeyStatus::Exhausted),
        )
        .filter(api_keys::Column::Id.eq(s.api_key_id))
        .filter(api_keys::Column::Status.eq(api_keys::KeyStatus::Enabled))
        .filter(api_keys::Column::BudgetLimit.is_not_null())
        .filter(
            Expr::col(api_keys::Column::BudgetUsed).gte(Expr::col(api_keys::Column::BudgetLimit)),
        )
        .exec(db)
        .await?;

    Ok(())
}
