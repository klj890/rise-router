//! 价格表（定价五要素④·显式单价）。按 (模型 × 分组) 存，group_id 为空=该模型默认价。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "prices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub model_id: i32,
    /// 为空 = 该模型对所有分组的默认价；非空 = 特定分组专属价
    pub group_id: Option<i32>,
    /// token / image / second / call（与 model.billing_unit 一致）
    pub billing_unit: String,
    pub currency: String,
    /// 显式单价（直观单位，无倍率）。按 billing_unit 结构不同，见 docs/data-model.md §5
    pub unit_prices: Json,
    pub valid_from: DateTimeWithTimeZone,
    pub valid_to: Option<DateTimeWithTimeZone>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
