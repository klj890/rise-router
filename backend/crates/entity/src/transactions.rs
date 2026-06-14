//! 资金流水（只追加）。amount 有符号（+充值/-消费）；balance_after 为该笔后的余额快照。
//! 无敏感字段，可派生 serde 直接作流水响应 DTO。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub org_id: i32,
    pub kind: TxnKind,
    /// 有符号金额：充值/退款为正，消费为负
    pub amount: Decimal,
    /// 该笔之后的余额快照
    pub balance_after: Decimal,
    /// 关联来源类型：usage_log / order / manual
    pub ref_type: Option<String>,
    pub ref_id: Option<i64>,
    pub memo: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

/// 资金流水类型（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum TxnKind {
    #[sea_orm(num_value = 1)]
    Recharge,
    #[sea_orm(num_value = 2)]
    Consume,
    #[sea_orm(num_value = 3)]
    Refund,
    #[sea_orm(num_value = 4)]
    Adjust,
    #[sea_orm(num_value = 5)]
    CreditRepay,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
