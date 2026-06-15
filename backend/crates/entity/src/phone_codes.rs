//! 短信验证码（注册/登录主通道）。**仅存 sha256 哈希**，不存明文；内部使用，不对外序列化。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "phone_codes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub phone: String,
    /// 验证码的 sha256 哈希（不存明文）
    pub code_hash: String,
    pub expires_at: DateTimeWithTimeZone,
    pub consumed_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    /// 该验证码的错误尝试次数；达上限即作废（防暴力枚举）
    pub attempts: i16,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
