//! 应收侧对账单（按月周期聚合 usage_logs 营收）。draft→locked 封账，锁定后只读。
//! 无敏感字段，可派生 serde 直接作对账单响应 DTO。
//! upstream_cost/gap 预留毛利字段，渠道成本字段未建前为空。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "reconciliations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 对账周期，形如 2026-06（YYYY-MM），全表唯一
    pub period: String,
    pub status: ReconStatus,
    /// 应收：SUM(usage_logs.charged_amount)
    pub total_revenue: Decimal,
    /// 调用数：COUNT(*)
    pub total_calls: i64,
    /// 渠道成本（毛利报表用）；渠道成本字段未建前为空
    pub upstream_cost: Option<Decimal>,
    /// 毛利缺口 = 应收 − 成本；成本未建前为空
    pub gap: Option<Decimal>,
    /// 模型级明细 [{model_id, revenue, calls}]
    pub detail: Option<Json>,
    pub generated_at: DateTimeWithTimeZone,
    /// 封账时间（locked 时回填）
    pub locked_at: Option<DateTimeWithTimeZone>,
}

/// 对账单状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum ReconStatus {
    #[sea_orm(num_value = 1)]
    Draft,
    #[sea_orm(num_value = 2)]
    Locked,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
