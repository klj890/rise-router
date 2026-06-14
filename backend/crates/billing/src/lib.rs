//! 计费与财务域：usage_logs 结算 + 流水查询（wallets/transactions/orders 留 M2 财务片）。
//!
//! 纯算费在 [`charge`]（无 DB，单测覆盖）；[`settle`] 是结算编排，网关 relay 复用。
//! 「看流水」端点按密钥 org 隔离（RLS 雏形），与 whoami 同样走 Bearer 鉴权。

mod charge;
mod settle;

pub use charge::{compute_charge, extract_token_usage};
pub use settle::{settle_chat, ChatSettlement};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::usage_logs;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_ping", get(|| async { "billing ok" }))
        .route("/usage", get(usage))
}

#[derive(Deserialize)]
struct UsageQuery {
    /// 返回条数上限（默认 50，封顶 200，避免拉全表）
    limit: Option<u64>,
    /// 游标：上一页最后一条记录的 id；返回 id < cursor 的更早记录（不传=最新一页）
    cursor: Option<i64>,
}

/// `GET /api/billing/usage`（Bearer 密钥）—— 看流水：仅返回本组织的调用计费明细，时间倒序。
/// 行级隔离按密钥归属的 org 强制过滤（RLS 雏形），客户碰不到他人流水。
async fn usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<UsageQuery>,
) -> AppResult<Json<Vec<usage_logs::Model>>> {
    let raw = rise_identity::bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let db = state.db()?;
    let ctx = rise_identity::verify_key(db, raw, chrono::Utc::now().fixed_offset()).await?;

    let limit = q.limit.unwrap_or(50).min(200);
    // 游标分页（按 id 倒序）：流水表无界增长，offset 深翻页要全扫前 N 行（性能），且高频追加会让
    // 行整体后移导致翻页重复（数据漂移）；游标走主键有序定位，耗时恒定且无漂移。id 单调 ≈ 时间序。
    let mut query = usage_logs::Entity::find().filter(usage_logs::Column::OrgId.eq(ctx.org_id));
    if let Some(cursor) = q.cursor {
        query = query.filter(usage_logs::Column::Id.lt(cursor));
    }
    let logs = query
        .order_by_desc(usage_logs::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    Ok(Json(logs))
}
