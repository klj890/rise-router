//! 销售代客开户（M3 片B）：销售为新客户创建组织 + 登录账号 + 首条归属。
//!
//! 数据域：销售（`crm.write` 无 `crm.read.all`）开的客户强制归属本人（`owner_sales_id` = 操作者）；
//! 管理员/财务（`crm.read.all`）或超管令牌可显式指定 `owner_sales_id` 代任意销售开户。
//! 事务建 org + user + 首条 active assignment，三者原子一致（避免半成品孤儿组织 / 无归属客户）。
//! 客户 user 无密码（`password_hash` 空），后续走手机号 + 短信验证码登录（复用 identity 登录）。

use axum::{extract::State, http::HeaderMap, Json};
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{
    customer_assignments,
    organizations::{self, OrgStatus, OrgType, RealnameStatus},
    users,
};
use sea_orm::{ActiveModelTrait, EntityTrait, Set, TransactionError, TransactionTrait};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct OnboardReq {
    /// 客户登录手机号（国情主通道；客户后续用手机号 + 短信登录）
    phone: String,
    /// 组织名称
    name: String,
    /// 组织类型：个人 / 企业（默认企业——销售开户多为企业客户）
    org_type: Option<OrgType>,
    /// 客户昵称（可选）
    nickname: Option<String>,
    /// 归属销售：仅全量权限（管理员/超管）可显式指定；销售本人忽略此字段强制归己
    owner_sales_id: Option<i32>,
}

#[derive(Serialize)]
pub struct OnboardResp {
    org: organizations::Model,
    /// 新建客户登录账号 id
    user_id: i32,
    /// 归属销售 id
    owner_sales_id: i32,
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

/// `POST /api/crm/customers`（crm.write）—— 销售代客开户。
///
/// 销售 → 客户归属本人；管理员/超管（crm.read.all）→ 必须指定 `owner_sales_id`（须为存在的 user）。
/// 事务：建 org + user + 首条 active assignment，原子提交。手机号已注册 → 400（一号不可两户）。
pub async fn create_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OnboardReq>,
) -> AppResult<Json<OnboardResp>> {
    let access =
        rise_identity::require_scoped(&state, &headers, "crm.write", "crm.read.all").await?;
    let db = state.db()?;

    // 归属决议：受限销售强制归己（忽略请求字段）；全量访问者须显式指定且目标销售须存在。
    let owner_sales_id = match access.owned_by() {
        Some(self_id) => self_id,
        None => {
            let sid = req
                .owner_sales_id
                .ok_or_else(|| AppError::BadRequest("owner_sales_id is required".into()))?;
            if users::Entity::find_by_id(sid).one(db).await?.is_none() {
                return Err(AppError::BadRequest("owner_sales_id not found".into()));
            }
            sid
        }
    };

    // 手机号格式 + 唯一（已注册 → 400，避免一号两户；users.phone 亦有唯一约束兜底）
    let phone = req.phone.trim().to_owned();
    if !rise_identity::valid_phone(&phone) {
        return Err(AppError::BadRequest("invalid phone number".into()));
    }
    if users::find_by_phone(db, &phone).await?.is_some() {
        return Err(AppError::BadRequest("phone already registered".into()));
    }
    let name = validate_name(&req.name)?;
    let org_type = req.org_type.unwrap_or(OrgType::Enterprise);
    let nickname = req
        .nickname
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let now = chrono::Utc::now().fixed_offset();

    // 事务：建 org + user + 首条 active assignment，原子（避免孤儿组织 / 无归属客户 / 无登录账号）。
    let resp = db
        .transaction::<_, OnboardResp, AppError>(move |txn| {
            Box::pin(async move {
                let org = organizations::ActiveModel {
                    name: Set(name),
                    org_type: Set(org_type),
                    status: Set(OrgStatus::Active),
                    realname_status: Set(RealnameStatus::Unverified),
                    owner_sales_id: Set(Some(owner_sales_id)),
                    ..Default::default()
                }
                .insert(txn)
                .await?;
                let user = users::ActiveModel {
                    org_id: Set(org.id),
                    phone: Set(phone),
                    nickname: Set(nickname),
                    status: Set(users::UserStatus::Enabled),
                    created_at: Set(now),
                    ..Default::default()
                }
                .insert(txn)
                .await?;
                customer_assignments::ActiveModel {
                    org_id: Set(org.id),
                    sales_id: Set(owner_sales_id),
                    assigned_at: Set(now),
                    active: Set(true),
                    ..Default::default()
                }
                .insert(txn)
                .await?;
                Ok(OnboardResp {
                    user_id: user.id,
                    owner_sales_id,
                    org,
                })
            })
        })
        .await
        .map_err(|e| match e {
            TransactionError::Connection(db) => AppError::Db(db),
            TransactionError::Transaction(app_err) => app_err,
        })?;

    tracing::info!(
        org_id = resp.org.id,
        owner_sales_id,
        "crm customer onboarded by sales"
    );
    Ok(Json(resp))
}
