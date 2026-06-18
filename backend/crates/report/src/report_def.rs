//! 定制报表定义端点：列表 / 创建 / 详情 / 删除。
//!
//! 报表只能基于数据集（创建时校验 dataset 存在 + 主体对其有访问权）。可见性：private 仅 owner，
//! 其余（role/org）对持 report.read 者可见（片A 简化；细粒度 role/org 共享留后续）。
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{datasets, report_definitions};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateReq {
    /// 基于的数据集 slug
    pub dataset_slug: String,
    pub name: String,
    /// 共享范围：private（默认）/ role / org
    pub visibility: Option<String>,
    /// 报表定义：{metrics,dimensions,filters,chart_type,refresh}
    #[serde(default)]
    pub config: serde_json::Value,
}

/// `GET /api/report/reports` —— 列出主体可见的报表（owner 自己的 + 非 private）。
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<report_definitions::Model>>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    if !principal.perms.contains("report.read") {
        return Err(AppError::Forbidden);
    }
    let db = state.db()?;
    let all = report_definitions::Entity::find()
        .order_by_desc(report_definitions::Column::Id)
        .all(db)
        .await?;
    let visible = all
        .into_iter()
        .filter(|r| {
            r.visibility != "private"
                || principal.role == "admin"
                || (r.owner_user_id.is_some() && r.owner_user_id == principal.user_id)
        })
        .collect();
    Ok(Json(visible))
}

/// `POST /api/report/reports` —— 创建定制报表（report.define）。
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<report_definitions::Model>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    if !principal.perms.contains("report.define") {
        return Err(AppError::Forbidden);
    }
    let name = req.name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let db = state.db()?;
    // 报表只能基于数据集 + 主体须对该数据集有访问权
    let ds = datasets::find_by_slug(db, &req.dataset_slug)
        .await?
        .ok_or_else(|| AppError::BadRequest("dataset not found".into()))?;
    if !principal.perms.contains(&ds.required_permission) {
        return Err(AppError::Forbidden);
    }
    let visibility = match req.visibility.as_deref() {
        None | Some("private") => "private",
        Some("role") => "role",
        Some("org") => "org",
        Some(_) => {
            return Err(AppError::BadRequest(
                "visibility must be private/role/org".into(),
            ))
        }
    };
    let model = report_definitions::ActiveModel {
        dataset_id: Set(ds.id),
        name: Set(name),
        owner_user_id: Set(principal.user_id),
        visibility: Set(visibility.to_owned()),
        config: Set(req.config),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(Json(model))
}

/// `GET /api/report/reports/{id}` —— 报表详情（owner 或非 private 可见）。
pub async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<report_definitions::Model>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    let db = state.db()?;
    let r = report_definitions::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let visible = r.visibility != "private"
        || principal.role == "admin"
        || (r.owner_user_id.is_some() && r.owner_user_id == principal.user_id);
    if !visible {
        return Err(AppError::NotFound); // 不泄露存在性
    }
    Ok(Json(r))
}

/// `DELETE /api/report/reports/{id}` —— 删除报表（owner 或 admin）。
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Json<serde_json::Value>> {
    let principal = rise_identity::resolve_principal(&state, &headers).await?;
    let db = state.db()?;
    let r = report_definitions::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    let owner = r.owner_user_id.is_some() && r.owner_user_id == principal.user_id;
    if !owner && principal.role != "admin" {
        return Err(AppError::Forbidden);
    }
    report_definitions::Entity::delete_by_id(id)
        .exec(db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
