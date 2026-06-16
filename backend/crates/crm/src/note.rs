//! 客户跟进记录：列表（org 内倒序游标分页）+ 新增。
//!
//! 数据域：销售仅能查看/记录自己名下客户的跟进（[`load_scoped_org`] 校验）。
//! 新增时 `author_id` 取操作者；超管令牌（无用户上下文）写入则为空。

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::customer_notes;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::Deserialize;

use crate::customer::load_scoped_org;

/// 跟进内容长度上限（content 列为 text，无库级限制；应用层防滥用）。
const MAX_NOTE_LEN: usize = 2000;

#[derive(Deserialize)]
pub struct ListQuery {
    /// 返回条数上限（默认 50，封顶 200）
    limit: Option<u64>,
    /// 游标：上一页最后一条 id；返回 id < cursor 的更早记录
    cursor: Option<i32>,
}

/// `GET /api/crm/customers/{org_id}/notes`（crm.read[.all]）—— 跟进记录，id 倒序游标分页。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<customer_notes::Model>>> {
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.read", "crm.read.all").await?;
    let db = state.db()?;
    load_scoped_org(db, org_id, &access).await?; // 数据域校验（404 不泄露存在性）

    let limit = q.limit.unwrap_or(50).min(200);
    let mut query = customer_notes::Entity::find().filter(customer_notes::Column::OrgId.eq(org_id));
    if let Some(cursor) = q.cursor {
        query = query.filter(customer_notes::Column::Id.lt(cursor));
    }
    let rows = query
        .order_by_desc(customer_notes::Column::Id)
        .limit(limit)
        .all(db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct CreateReq {
    content: String,
}

/// 校验跟进内容：trim 后非空、长度封顶。
fn validate_content(raw: &str) -> AppResult<String> {
    let content = raw.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("content is required".into()));
    }
    // 先按字节快速预检：UTF-8 每字符 ≥1 字节，故 bytes > MAX*4 必然超长，
    // 借短路避免对超大输入（恶意数 MB 文本）做 O(N) 的 chars().count() —— 防 CPU 耗尽。
    if content.len() > MAX_NOTE_LEN * 4 || content.chars().count() > MAX_NOTE_LEN {
        return Err(AppError::BadRequest(format!(
            "content too long (max {MAX_NOTE_LEN})"
        )));
    }
    Ok(content.to_owned())
}

/// `POST /api/crm/customers/{org_id}/notes`（crm.write）—— 新增跟进，author_id 取操作者。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org_id): Path<i32>,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<customer_notes::Model>> {
    // 写权限 + 归属边界：销售（crm.write 无 crm.read.all）仅能给自己名下客户记跟进。
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.write", "crm.read.all").await?;
    let db = state.db()?;
    load_scoped_org(db, org_id, &access).await?;

    let content = validate_content(&req.content)?;
    let note = customer_notes::ActiveModel {
        org_id: Set(org_id),
        author_id: Set(access.actor_id()),
        content: Set(content),
        created_at: Set(chrono::Utc::now().fixed_offset()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(Json(note))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_content_trims_and_bounds() {
        assert_eq!(validate_content("  跟进了客户 ").unwrap(), "跟进了客户");
        assert!(validate_content("   ").is_err());
        assert!(validate_content(&"x".repeat(MAX_NOTE_LEN + 1)).is_err());
        assert!(validate_content(&"汉".repeat(MAX_NOTE_LEN)).is_ok());
    }
}
