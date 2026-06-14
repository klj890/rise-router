//! 充值订单（mock 支付）。客户/销售发起充值 → Pending；mock 确认成功 → Paid 并入账钱包。
//! 无敏感字段，可派生 serde 直接作订单响应 DTO。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "orders")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    /// 发起销售（CRM 软引用，可空；users 表落地前不建 FK）
    pub created_by_sales_id: Option<i32>,
    pub amount: Decimal,
    /// 支付渠道：mock / wechat / alipay / transfer …
    pub pay_channel: String,
    /// 第三方支付交易号（mock 阶段可空、不唯一）
    pub trade_no: Option<String>,
    pub status: OrderStatus,
    pub memo: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    /// 支付确认时间（Paid 时回填）
    pub paid_at: Option<DateTimeWithTimeZone>,
}

/// 订单状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum OrderStatus {
    #[sea_orm(num_value = 1)]
    Pending,
    #[sea_orm(num_value = 2)]
    Paid,
    #[sea_orm(num_value = 3)]
    Failed,
    #[sea_orm(num_value = 4)]
    Refunded,
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
