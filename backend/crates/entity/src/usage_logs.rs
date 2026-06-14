//! 调用计费流水（只追加，对应 new-api logs）。
//!
//! 与携带凭据的实体不同，本表无敏感字段，**可安全派生 serde 直接作流水响应 DTO**。
//! `group_slug` 是计费当下的分组快照（事后改 org 分组不影响历史账）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "usage_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub org_id: i32,
    pub user_id: Option<i32>,
    pub api_key_id: i32,
    pub app_id: Option<i32>,
    pub model_id: i32,
    pub channel_id: i32,
    /// 计费快照分组（org 无分组时为空 = 默认价）
    pub group_slug: Option<String>,
    pub request_id: Option<String>,
    pub billing_unit: String,
    /// 用量 {input,output} / {seconds,resolution} 等
    pub quantity: Json,
    /// 折前金额
    pub base_amount: Decimal,
    /// 命中折扣明细（可追溯）
    pub discount_detail: Option<Json>,
    /// 实扣金额（已应用 percentage 折扣）
    pub charged_amount: Decimal,
    /// 渠道成本（毛利报表用）；渠道成本字段未建前为空
    pub cost_amount: Option<Decimal>,
    pub latency_ms: Option<i32>,
    pub is_stream: bool,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
