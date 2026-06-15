//! 权限点（由 App/内部模块声明注入）。code 唯一，如 `pricing.manage`。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "permissions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub code: String,
    /// 来源模块（app_id FK 待 apps 表落地后补）
    pub module: Option<String>,
    pub description: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 按 code 查权限点。
pub async fn find_by_code<C: ConnectionTrait>(db: &C, code: &str) -> Result<Option<Model>, DbErr> {
    Entity::find().filter(Column::Code.eq(code)).one(db).await
}
