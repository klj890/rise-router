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
    /// 密钥/多 key 轮询配置（加密留待后续，现存 jsonb）。
    /// serde(skip)：上游凭据绝不随 Model 序列化进 API 响应/日志，避免密钥泄露。
    #[serde(skip)]
    pub credentials: Json,
    /// 协议族内消化厂商 quirk 的配置开关
    pub adapter_config: Option<Json>,
    pub priority: i32,
    pub weight: i32,
    pub status: ChannelStatus,
}

/// 渠道状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum ChannelStatus {
    #[sea_orm(num_value = 1)]
    Enabled,
    #[sea_orm(num_value = 2)]
    Disabled,
    #[sea_orm(num_value = 3)]
    CircuitBroken,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
