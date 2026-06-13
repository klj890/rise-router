//! 模型能力目录（定价五要素②·纯能力，无价格）。display_name 用 i18n JSONB。
use sea_orm::entity::prelude::*;
use sea_orm::{ConnectionTrait, QueryFilter};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "models")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub slug: String,
    /// 本地化显示名 {"zh-CN": "...", "en-US": "..."}
    pub display_name_i18n: Json,
    /// chat / embedding / image / video / audio / rerank
    pub modality: String,
    /// sync_stream / async_task
    pub invocation: String,
    /// 计费量纲：token / image / second / call
    pub billing_unit: String,
    pub capabilities: Option<Json>,
    pub status: ModelStatus,
}

/// 模型上架状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum ModelStatus {
    #[sea_orm(num_value = 1)]
    Listed,
    #[sea_orm(num_value = 2)]
    Delisted,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 按 slug 查"上架"模型（gateway/pricing 共用，去重并统一过滤下架模型）。
pub async fn find_listed_by_slug<C: ConnectionTrait>(
    db: &C,
    slug: &str,
) -> Result<Option<Model>, DbErr> {
    Entity::find()
        .filter(Column::Slug.eq(slug))
        .filter(Column::Status.eq(ModelStatus::Listed))
        .one(db)
        .await
}
