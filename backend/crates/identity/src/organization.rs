//! 组织（账户与计费主体；个人=org-of-one）管理 CRUD。
//!
//! 组织是计费主体：钱包/分组/密钥都挂这里。实体无密钥、已派生 serde，响应直接返回 Model。
//! `group_id`（商业分组，定价档位）与 `owner_sales_id`（CRM 归属）是**可清空**字段：
//! 用 double_option 区分「不传=不变 / 传 null=清空 / 传值=设置」——移组织回默认价、解绑销售都是真实运营。
//! 删除遇到被 api_keys(FK CASCADE)/wallets/orders 引用时 400——杜绝级联删账，请改用停用（status=Suspended）。

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{api_keys, groups, orders, organizations, wallets};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Deserializer};

use organizations::{OrgStatus, OrgType, RealnameStatus};

/// serde 双层 Option：区分字段缺省（None）/ 显式 null（Some(None)）/ 有值（Some(Some(v)))。
/// 用于可清空字段的部分更新。
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(de)?))
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

async fn ensure_group_exists(db: &sea_orm::DatabaseConnection, group_id: i32) -> AppResult<()> {
    if groups::Entity::find_by_id(group_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::BadRequest("group_id not found".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    org_type: OrgType,
    /// 商业分组（定价档位）；缺省 = 按默认价计费
    group_id: Option<i32>,
    status: Option<OrgStatus>,
    realname_status: Option<RealnameStatus>,
    owner_sales_id: Option<i32>,
}

/// `POST /api/identity/organizations`（admin）—— 新建组织。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<organizations::Model>> {
    crate::require(&state, &headers, "identity.manage").await?;
    let db = state.db()?;

    let name = validate_name(&req.name)?;
    if let Some(g) = req.group_id {
        ensure_group_exists(db, g).await?;
    }

    let m = organizations::ActiveModel {
        name: Set(name),
        org_type: Set(req.org_type),
        group_id: Set(req.group_id),
        status: Set(req.status.unwrap_or(OrgStatus::Active)),
        realname_status: Set(req.realname_status.unwrap_or(RealnameStatus::Unverified)),
        owner_sales_id: Set(req.owner_sales_id),
        ..Default::default()
    };
    let m = m.insert(db).await?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// 可选按商业分组过滤
    group_id: Option<i32>,
    /// 可选按归属销售过滤（CRM）
    owner_sales_id: Option<i32>,
}

/// `GET /api/identity/organizations`（admin）—— 列出组织，可按 group/sales 过滤，id 升序。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<organizations::Model>>> {
    crate::require(&state, &headers, "identity.manage").await?;
    let db = state.db()?;
    let mut query = organizations::Entity::find();
    if let Some(g) = q.group_id {
        query = query.filter(organizations::Column::GroupId.eq(g));
    }
    if let Some(s) = q.owner_sales_id {
        query = query.filter(organizations::Column::OwnerSalesId.eq(s));
    }
    let rows = query
        .order_by_asc(organizations::Column::Id)
        .all(db)
        .await?;
    Ok(Json(rows))
}

/// `GET /api/identity/organizations/{id}`（admin）—— 取单个组织。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<organizations::Model>> {
    crate::require(&state, &headers, "identity.manage").await?;
    let db = state.db()?;
    let m = organizations::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(m))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    org_type: Option<OrgType>,
    /// 不传=不变；null=清空回默认价；值=设组（须存在）
    #[serde(default, deserialize_with = "double_option")]
    group_id: Option<Option<i32>>,
    status: Option<OrgStatus>,
    realname_status: Option<RealnameStatus>,
    /// 不传=不变；null=解绑销售；值=设归属
    #[serde(default, deserialize_with = "double_option")]
    owner_sales_id: Option<Option<i32>>,
}

/// `PUT /api/identity/organizations/{id}`（admin）—— 部分更新。
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<organizations::Model>> {
    crate::require(&state, &headers, "identity.manage").await?;
    let db = state.db()?;

    let existing = organizations::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: organizations::ActiveModel = existing.into();

    if let Some(name) = req.name {
        am.name = Set(validate_name(&name)?);
    }
    if let Some(t) = req.org_type {
        am.org_type = Set(t);
    }
    if let Some(g) = req.group_id {
        if let Some(gid) = g {
            ensure_group_exists(db, gid).await?;
        }
        am.group_id = Set(g);
    }
    if let Some(s) = req.status {
        am.status = Set(s);
    }
    if let Some(r) = req.realname_status {
        am.realname_status = Set(r);
    }
    if let Some(owner) = req.owner_sales_id {
        am.owner_sales_id = Set(owner);
    }

    let m = am.update(db).await?;
    Ok(Json(m))
}

/// `DELETE /api/identity/organizations/{id}`（admin）—— 删除组织。
/// 被 api_keys(CASCADE)/wallets/orders 引用时 400：组织是计费主体，硬删会级联删密钥/钱包/账，
/// 几乎从不是正确操作；请改用停用（PUT status=Suspended）。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    crate::require(&state, &headers, "identity.manage").await?;
    let db = state.db()?;

    if organizations::Entity::find_by_id(id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }
    let key_refs = api_keys::Entity::find()
        .filter(api_keys::Column::OrgId.eq(id))
        .count(db)
        .await?;
    let wallet_refs = wallets::Entity::find()
        .filter(wallets::Column::OrgId.eq(id))
        .count(db)
        .await?;
    let order_refs = orders::Entity::find()
        .filter(orders::Column::OrgId.eq(id))
        .count(db)
        .await?;
    if key_refs > 0 || wallet_refs > 0 || order_refs > 0 {
        return Err(AppError::BadRequest(format!(
            "organization is referenced by {key_refs} key(s), {wallet_refs} wallet(s), {order_refs} order(s); suspend it instead"
        )));
    }
    organizations::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_trims_and_bounds() {
        assert_eq!(validate_name("  鹏博士 ").unwrap(), "鹏博士");
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"x".repeat(129)).is_err());
    }

    #[test]
    fn double_option_distinguishes_absent_null_value() {
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "double_option")]
            g: Option<Option<i32>>,
        }
        let absent: T = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.g, None); // 不传 = 不变
        let null: T = serde_json::from_str(r#"{"g": null}"#).unwrap();
        assert_eq!(null.g, Some(None)); // null = 清空
        let val: T = serde_json::from_str(r#"{"g": 7}"#).unwrap();
        assert_eq!(val.g, Some(Some(7))); // 值 = 设置
    }
}
