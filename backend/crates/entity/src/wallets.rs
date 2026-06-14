//! 账户钱包（挂组织，一对一）。无敏感字段，可派生 serde 直接作响应 DTO。
//! 可用额度 = balance + credit_limit − frozen（见 rise-billing 的 wallet_available）。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "wallets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub org_id: i32,
    /// 余额（真实的钱，消费扣减/充值增加）
    pub balance: Decimal,
    /// 授信额度（企业后付费，可透支至 -credit_limit）
    pub credit_limit: Decimal,
    /// 预扣冻结额（占用但未结算）
    pub frozen: Decimal,
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
