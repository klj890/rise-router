//! 路由表（Ability，model↔channel）。剥离 group/价格，只管能力可达与负载。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "model_channels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub model_id: i32,
    pub channel_id: i32,
    /// 上游真实模型名（模型映射）
    pub upstream_model_name: String,
    pub enabled: bool,
    /// 覆盖渠道默认优先级（空=用 channel.priority）
    pub priority: Option<i32>,
    /// 覆盖渠道默认权重（空=用 channel.weight）
    pub weight: Option<i32>,
    /// 渠道成本价（按 billing_unit）；售价在 prices，二者分离
    pub cost_price: Option<Json>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::models::Entity",
        from = "Column::ModelId",
        to = "super::models::Column::Id"
    )]
    Model,
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id"
    )]
    Channel,
}

impl Related<super::models::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Model.def()
    }
}

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
