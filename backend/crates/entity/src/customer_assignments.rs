//! CRM 客户归属变更历史（M3 片 A）。无敏感字段，可派生 serde 直接作响应 DTO。
//! `organizations.owner_sales_id` 为当前归属真相源；本表是变更轨迹（业绩归因 / 审计）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "customer_assignments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    /// 归属销售（users.id 软引用）
    pub sales_id: i32,
    pub assigned_at: DateTimeWithTimeZone,
    /// 当前归属标记：改派时旧行置 false
    pub active: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_delete = "Cascade"
    )]
    Organization,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
