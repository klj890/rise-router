//! 用户（登录主体，组织成员）。手机号为国情主注册通道。
//!
//! **禁止直接用作响应 DTO**——携带 password_hash 凭据。故意不派生 serde：
//! 对外用 identity 域的专用 `UserView`（不含 password_hash）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    #[sea_orm(unique)]
    pub phone: String,
    pub email: Option<String>,
    /// argon2/bcrypt（手机号+短信为主通道，可空）
    pub password_hash: Option<String>,
    pub nickname: Option<String>,
    pub status: UserStatus,
    pub last_login_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}

/// 用户状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum UserStatus {
    #[sea_orm(num_value = 1)]
    Enabled,
    #[sea_orm(num_value = 2)]
    Disabled,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 按手机号查用户（登录/注册判重）。
pub async fn find_by_phone<C: ConnectionTrait>(
    db: &C,
    phone: &str,
) -> Result<Option<Model>, DbErr> {
    Entity::find().filter(Column::Phone.eq(phone)).one(db).await
}
