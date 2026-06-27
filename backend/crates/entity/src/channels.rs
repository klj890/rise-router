//! 渠道（定价五要素③相关·纯接入；成本与售价分离）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 持久化实体。**禁止直接用作 API 请求/响应 DTO**——它携带 `credentials` 密钥。
/// 故意不派生 serde：渠道 CRUD 接口须定义专用 DTO（响应 DTO 不含 credentials；
/// 更新 DTO 用 `Option<Json>`，仅 `Some` 时改库），避免密钥泄露与空值覆盖。
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
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
    pub status: ChannelStatus,
    /// 最近一次连通性测试的响应耗时（ms）；未测过为 None。
    pub response_time: Option<i32>,
    /// 最近一次测试时间。
    pub test_time: Option<DateTimeWithTimeZone>,
    /// 渠道测试默认模型（空时由 test 端点取该渠道 model_channels 首条）。
    pub test_model: Option<String>,
    /// 是否允许被自动禁用（健康探活/转发错误触发）。
    pub auto_ban: bool,
    /// 自动禁用原因（status=CircuitBroken 时填，便于排查）。
    pub disabled_reason: Option<String>,
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
