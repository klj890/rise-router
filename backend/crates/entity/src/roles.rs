//! 角色（RBAC 能力束）。挂 user（经 user_roles），决定能做什么。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "roles")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub slug: String,
    pub name: String,
    /// 内置角色不可删
    pub is_builtin: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 按 slug 查角色。
pub async fn find_by_slug<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Option<Model>, DbErr> {
    Entity::find().filter(Column::Slug.eq(slug)).one(db).await
}
