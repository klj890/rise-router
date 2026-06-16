//! CRM 客户跟进记录（M3 片 A）。无敏感字段，可派生 serde 直接作响应 DTO。
//! org_id=客户（组织，FK cascade）；author_id=记录人（users.id 软引用，可空）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "customer_notes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    /// 记录人（users.id 软引用，可空：超管令牌创建时为空）
    pub author_id: Option<i32>,
    pub content: String,
    pub created_at: DateTimeWithTimeZone,
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
