//! 渠道（定价五要素③相关·纯接入；成本与售价分离）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "channels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    /// 协议族：openai_compatible / anthropic / gemini / task_*
    pub protocol_adapter: String,
    pub base_url: String,
    /// 密钥/多 key 轮询配置（加密留待后续，现存 jsonb）
    pub credentials: Json,
    /// 协议族内消化厂商 quirk 的配置开关
    pub adapter_config: Option<Json>,
    pub priority: i32,
    pub weight: i32,
    /// 1=启用 2=手动禁用 3=自动熔断
    pub status: i16,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
