//! 任务控制台监控视图（M5a 片D）：跨租户列表 + 取消，管理令牌（X-Admin-Token）守卫。
//!
//! `/v1/tasks` 是按密钥鉴权的对外 API（无 list）；控制台是 ops 监控视图，故走 admin 网关的
//! 跨 org 只读列表（对齐 billing/admin_read 范式）。提交仍由 API 消费方用密钥经 `/v1/tasks`。

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use rise_core::{admin_guard, AppError, AppResult, AppState};
use rise_entity::{organizations, tasks};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ListQuery {
    /// 返回条数上限（默认 100，封顶 500）
    limit: Option<u64>,
}

/// 任务行（跨租户）：任务字段 + 租户名。
#[derive(Serialize)]
pub struct TaskRow {
    #[serde(flatten)]
    task: tasks::Model,
    org_name: String,
}

async fn org_names(
    db: &sea_orm::DatabaseConnection,
    ids: &[i32],
) -> AppResult<HashMap<i32, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let orgs = organizations::Entity::find()
        .filter(organizations::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    Ok(orgs.into_iter().map(|o| (o.id, o.name)).collect())
}

/// `GET /api/task/admin/tasks?limit=`（管理令牌）—— 跨租户任务列表，按 id 倒序。
pub async fn list(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<TaskRow>>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let limit = core::cmp::min(q.limit.unwrap_or(100), 500);

    let list = tasks::Entity::find()
        .order_by_desc(tasks::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    // 去重 org_id 后查租户名（避免 IN 冗余）。
    let mut org_ids: Vec<i32> = list.iter().map(|t| t.org_id).collect();
    org_ids.sort_unstable();
    org_ids.dedup();
    let names = org_names(db, &org_ids).await?;
    let rows = list
        .into_iter()
        .map(|t| TaskRow {
            org_name: names
                .get(&t.org_id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", t.org_id)),
            task: t,
        })
        .collect();
    Ok(Json(rows))
}

/// `POST /api/task/admin/tasks/{id}/cancel`（管理令牌）—— 控制台取消任务（跨租户，含上游取消）。
pub async fn cancel(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<Json<tasks::Model>> {
    admin_guard(&state, &headers)?;
    let db = state.db()?;
    let now = chrono::Utc::now().fixed_offset();

    // 原子条件取消（Queued/Running → Cancelled），跨 org（管理视图无归属过滤）。
    let res = tasks::Entity::update_many()
        .filter(tasks::Column::Id.eq(id))
        .filter(
            tasks::Column::Status.is_in([tasks::TaskStatus::Queued, tasks::TaskStatus::Running]),
        )
        .col_expr(
            tasks::Column::Status,
            Expr::value(tasks::TaskStatus::Cancelled),
        )
        .col_expr(tasks::Column::FinishedAt, Expr::value(now))
        .col_expr(tasks::Column::UpdatedAt, Expr::value(now))
        .exec(db)
        .await?;

    let latest = tasks::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;

    // 未发生转换且当前非 Cancelled（已 Succeeded/Failed，或与 poller 竞态抢先终结）→
    // 返回 400，避免前端误报「已取消」后刷新又见「成功」的不一致。重复取消（已是 Cancelled）幂等放行。
    if res.rows_affected == 0 && latest.status != tasks::TaskStatus::Cancelled {
        return Err(AppError::BadRequest(format!(
            "任务当前状态为 {:?}，无法取消",
            latest.status
        )));
    }

    // 真正发生取消转换且已提交上游 → 后台尽力取消上游，闭合泄漏。
    if res.rows_affected == 1 && latest.vendor_task_id.is_some() {
        crate::worker::spawn_upstream_cancel(state.clone(), latest.clone());
    }
    Ok(Json(latest))
}
