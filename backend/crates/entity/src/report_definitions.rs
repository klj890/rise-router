//! 定制报表定义（M4 监控报表域）：可保存的报表 = 数据集 + 指标 + 维度 + 过滤 + 图表类型。
//!
//! 报表**只能基于数据集**（`dataset_id` FK）搭建，碰不到原始表。`visibility` 控制共享范围
//! （private 仅自己 / role 同角色 / org 同组织）。`config` 存选定的指标/维度/过滤/图表类型/刷新周期。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "report_definitions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub dataset_id: i32,
    pub name: String,
    /// 创建者 user_id（超管令牌创建时为空）
    pub owner_user_id: Option<i32>,
    /// 共享范围：private / role / org
    pub visibility: String,
    /// 报表定义：{metrics,dimensions,filters,chart_type,refresh}
    pub config: Json,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::datasets::Entity",
        from = "Column::DatasetId",
        to = "super::datasets::Column::Id",
        on_delete = "Cascade"
    )]
    Dataset,
}

impl Related<super::datasets::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Dataset.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
