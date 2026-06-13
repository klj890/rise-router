//! 折扣表（定价五要素⑤·独立、显式、可叠加）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "discounts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    /// global / group / model / org / model_group
    pub scope: String,
    /// scope=org 时（FK 待 organizations 表落地后补约束）
    pub target_org_id: Option<i32>,
    pub target_group_id: Option<i32>,
    pub target_model_id: Option<i32>,
    /// percentage（打折，value=0.9 即九折）/ fixed（减额，结算期生效）
    pub kind: String,
    pub value: Decimal,
    pub stackable: bool,
    pub priority: i32,
    pub valid_from: DateTimeWithTimeZone,
    pub valid_to: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
