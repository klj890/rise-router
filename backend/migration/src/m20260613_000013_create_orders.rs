//! 充值订单（mock 支付）。客户/销售发起 → Pending；mock 确认 → Paid 并入账钱包。
//! 软引用销售（created_by_sales_id 不建 FK，users 表落地前）；org 连带删（cascade）。
//! mock 阶段 trade_no 不设唯一约束（真实支付网关接入时再补幂等键）。
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
                    .table(Orders::Table)
                    .if_not_exists()
                    .col(pk_auto(Orders::Id))
                    .col(integer(Orders::OrgId))
                    // 软引用销售（不建 FK，users 表落地前）
                    .col(integer_null(Orders::CreatedBySalesId))
                    .col(decimal_len(Orders::Amount, 18, 8))
                    .col(string_len(Orders::PayChannel, 32))
                    // mock 阶段不唯一；真实网关接入时再加唯一键做幂等
                    .col(string_len_null(Orders::TradeNo, 128))
                    // 1待支付 2已支付 3失败 4已退款
                    .col(small_integer(Orders::Status).default(1))
                    .col(string_len_null(Orders::Memo, 256))
                    .col(
                        timestamp_with_time_zone(Orders::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .col(timestamp_with_time_zone_null(Orders::PaidAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_orders_org")
                            .from(Orders::Table, Orders::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_orders_org_id")
                    .table(Orders::Table)
                    .col(Orders::OrgId)
                    .col(Orders::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Orders::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Orders {
    Table,
    Id,
    OrgId,
    CreatedBySalesId,
    Amount,
    PayChannel,
    TradeNo,
    Status,
    Memo,
    CreatedAt,
    PaidAt,
}
