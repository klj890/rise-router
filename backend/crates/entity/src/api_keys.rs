//! 虚拟密钥（借鉴 LiteLLM virtual key）。仅存哈希，预算/模型白名单/过期挂在 key 上。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 持久化实体。**禁止直接用作 API 请求/响应 DTO**——key_hash 是凭据。
/// 故意不派生 serde：密钥接口须用专用 DTO（创建时只回明文一次，列表不含 hash）。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    /// 可空（组织级密钥，不绑定具体用户）；FK 待 users 表落地后补
    pub user_id: Option<i32>,
    /// 可空（App 专用密钥，用量挂 App）；FK 待 apps 表落地后补
    pub app_id: Option<i32>,
    #[sea_orm(unique)]
    pub key_hash: String,
    pub name: String,
    /// 模型白名单（空=不限）
    pub allowed_models: Option<Json>,
    /// 预算上限（空=不限）；命中返回 429
    pub budget_limit: Option<Decimal>,
    pub budget_used: Decimal,
    /// 过期时间（空=永不过期）
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub status: KeyStatus,
}

/// 密钥状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum KeyStatus {
    #[sea_orm(num_value = 1)]
    Enabled,
    #[sea_orm(num_value = 2)]
    Disabled,
    #[sea_orm(num_value = 3)]
    Exhausted,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 按 key_hash 查密钥（鉴权热路径，命中唯一索引）。
pub async fn find_by_key_hash<C: ConnectionTrait>(
    db: &C,
    key_hash: &str,
) -> Result<Option<Model>, DbErr> {
    Entity::find()
        .filter(Column::KeyHash.eq(key_hash))
        .one(db)
        .await
}
