//! 模型能力目录（定价五要素②·纯能力，无价格）。display_name 用 i18n JSONB。
use sea_orm::entity::prelude::*;
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
    /// 1=上架 2=下架
    pub status: i16,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
