//! 用户分组（定价五要素·纯分类，不含价格字段）管理 CRUD。
//!
//! 分组只是商业档位标签（企业客户/套餐档/销售渠道），价格在 prices（模型×分组），二者解耦。
//! 实体无密钥、已派生 serde，响应直接返回 [`groups::Model`]。`slug` 唯一（建表 UK）：
//! 创建/改名前查重防 500。删除遇到被 organizations（FK SET NULL，会静默把客户改回默认价）
//! 或 prices（FK CASCADE，会连带删该组价格）引用时 400——先让管理员清依赖，防静默改计费。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{groups, organizations, prices};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::Deserialize;

fn validate_slug(slug: &str) -> AppResult<String> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Err(AppError::BadRequest("slug is required".into()));
    }
    if slug.chars().count() > 64 {
        return Err(AppError::BadRequest("slug too long (max 64)".into()));
    }
    Ok(slug.to_owned())
}

fn validate_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if name.chars().count() > 128 {
        return Err(AppError::BadRequest("name too long (max 128)".into()));
    }
    Ok(name.to_owned())
}

/// description 清洗：trim 后空 → None（清空）；超长 400；否则 Some(value)。
fn clean_description(desc: Option<String>) -> AppResult<Option<String>> {
    let d = desc
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Some(ref s) = d {
        if s.chars().count() > 256 {
            return Err(AppError::BadRequest(
                "description too long (max 256)".into(),
            ));
        }
    }
    Ok(d)
}

async fn find_by_slug(
    db: &sea_orm::DatabaseConnection,
    slug: &str,
) -> AppResult<Option<groups::Model>> {
    Ok(groups::Entity::find()
        .filter(groups::Column::Slug.eq(slug))
        .one(db)
        .await?)
}

#[derive(Deserialize)]
pub struct CreateReq {
    slug: String,
    name: String,
    description: Option<String>,
}

/// `POST /api/pricing/groups`（admin）—— 新建商业分组。slug 查重 → 400。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<groups::Model>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;

    let slug = validate_slug(&req.slug)?;
    let name = validate_name(&req.name)?;
    let description = clean_description(req.description)?;

    if find_by_slug(db, &slug).await?.is_some() {
        return Err(AppError::BadRequest(format!(
            "slug '{slug}' already exists"
        )));
    }

    let m = groups::ActiveModel {
        slug: Set(slug),
        name: Set(name),
        description: Set(description),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(m))
}

/// `GET /api/pricing/groups`（admin）—— 列出全部分组，id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<groups::Model>>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;
    let rows = groups::Entity::find()
        .order_by_asc(groups::Column::Id)
        .all(db)
        .await?;
    Ok(Json(rows))
}

/// `GET /api/pricing/groups/{id}`（admin）—— 取单个分组。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<groups::Model>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;
    let m = groups::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    slug: Option<String>,
    name: Option<String>,
    /// Some("") 清空 description；Some(非空) 设值；None 不变
    description: Option<String>,
}

/// `PUT /api/pricing/groups/{id}`（admin）—— 部分更新。改 slug 时跨行查重。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<groups::Model>> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;

    let existing = groups::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: groups::ActiveModel = existing.into();

    if let Some(slug) = req.slug {
        let slug = validate_slug(&slug)?;
        if let Some(other) = find_by_slug(db, &slug).await? {
            if other.id != id {
                return Err(AppError::BadRequest(format!(
                    "slug '{slug}' already exists"
                )));
            }
        }
        am.slug = Set(slug);
    }
    if let Some(name) = req.name {
        am.name = Set(validate_name(&name)?);
    }
    if let Some(desc) = req.description {
        am.description = Set(clean_description(Some(desc))?);
    }

    let m = am.update(db).await?;
    Ok(Json(m))
}

/// `DELETE /api/pricing/groups/{id}`（admin）—— 删除分组。
/// 被 organizations（FK SET NULL → 静默改回默认价）或 prices（FK CASCADE → 连带删价）引用时 400。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    rise_identity::require(&state, &headers, "pricing.manage").await?;
    let db = state.db()?;

    if groups::Entity::find_by_id(id).one(db).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let org_refs = organizations::Entity::find()
        .filter(organizations::Column::GroupId.eq(id))
        .count(db)
        .await?;
    let price_refs = prices::Entity::find()
        .filter(prices::Column::GroupId.eq(id))
        .count(db)
        .await?;
    if org_refs > 0 || price_refs > 0 {
        return Err(AppError::BadRequest(format!(
            "group is referenced by {org_refs} org(s) and {price_refs} price(s); reassign or remove them first"
        )));
    }
    groups::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_name_trim_and_bound() {
        assert_eq!(validate_slug("  vip ").unwrap(), "vip");
        assert!(validate_slug("   ").is_err());
        assert!(validate_slug(&"x".repeat(65)).is_err());
        assert_eq!(validate_name("  企业客户 ").unwrap(), "企业客户");
        assert!(validate_name("  ").is_err());
    }

    #[test]
    fn description_blank_becomes_none_and_too_long_rejected() {
        assert_eq!(clean_description(None).unwrap(), None);
        assert_eq!(clean_description(Some("   ".into())).unwrap(), None);
        assert_eq!(
            clean_description(Some(" 高优先 ".into())).unwrap(),
            Some("高优先".to_owned())
        );
        assert!(clean_description(Some("x".repeat(257))).is_err());
    }
}
