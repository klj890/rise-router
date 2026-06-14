//! 发票（M2 片 D）。客户/管理员对已充值金额申请开票：pending→issued，可 void 作废。
//! 区分普票/专票（专票须有税号，由 billing 层校验）。order_id 软引用充值订单（不建 FK）。
//! 无敏感字段，可派生 serde 直接作发票响应 DTO。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "invoices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub org_id: i32,
    /// 软引用某笔充值订单（orders.id；不建 FK，仅作审计追溯，可空）
    pub order_id: Option<i32>,
    pub invoice_type: InvoiceType,
    /// 发票抬头
    pub title: String,
    /// 纳税人识别号（专票必填，普票可空）
    pub tax_no: Option<String>,
    pub amount: Decimal,
    pub status: InvoiceStatus,
    pub memo: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    /// 开票时间（issued 时回填）
    pub issued_at: Option<DateTimeWithTimeZone>,
}

/// 发票类型（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum InvoiceType {
    /// 普通发票（增值税普通发票）
    #[sea_orm(num_value = 1)]
    Normal,
    /// 专用发票（增值税专用发票，必须有税号）
    #[sea_orm(num_value = 2)]
    Special,
}

/// 发票状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum InvoiceStatus {
    /// 待开票
    #[sea_orm(num_value = 1)]
    Pending,
    /// 已开票
    #[sea_orm(num_value = 2)]
    Issued,
    /// 已作废
    #[sea_orm(num_value = 3)]
    Voided,
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
