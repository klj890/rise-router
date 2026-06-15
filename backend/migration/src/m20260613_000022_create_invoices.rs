//! 发票（M2 片 D）。客户/管理员对已充值金额申请开票：pending→issued，可 void 作废。
//! org 必填连带删（cascade）；order_id 软引用某笔充值订单（**不建 FK**）——发票是法定财务凭证，
//! 须独立于订单生命周期留存，订单删除不应连带删发票，故仅存 id 作审计追溯线索。
//! 真实税务系统对接 / PDF / 红冲 / 批量开票留后续片。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000007_create_organizations::Organizations;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Invoices::Table)
                    .if_not_exists()
                    .col(pk_auto(Invoices::Id))
                    .col(integer(Invoices::OrgId))
                    // 软引用某笔充值订单（不建 FK，发票须独立于订单生命周期留存）
                    .col(integer_null(Invoices::OrderId))
                    // 1=普票 2=专票
                    .col(small_integer(Invoices::InvoiceType).default(1))
                    // 抬头
                    .col(string_len(Invoices::Title, 128))
                    // 税号（专票必填，由应用层校验）
                    .col(string_len_null(Invoices::TaxNo, 64))
                    .col(decimal_len(Invoices::Amount, 18, 8))
                    // 1=pending 2=issued 3=voided
                    .col(small_integer(Invoices::Status).default(1))
                    .col(string_len_null(Invoices::Memo, 256))
                    .col(
                        timestamp_with_time_zone(Invoices::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    // 开票时间（issued 时回填）
                    .col(timestamp_with_time_zone_null(Invoices::IssuedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_invoices_org")
                            .from(Invoices::Table, Invoices::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // 游标分页（org 隔离 + id 倒序）
        manager
            .create_index(
                Index::create()
                    .name("idx_invoices_org_id")
                    .table(Invoices::Table)
                    .col(Invoices::OrgId)
                    .col(Invoices::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Invoices::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Invoices {
    Table,
    Id,
    OrgId,
    OrderId,
    InvoiceType,
    Title,
    TaxNo,
    Amount,
    Status,
    Memo,
    CreatedAt,
    IssuedAt,
}
