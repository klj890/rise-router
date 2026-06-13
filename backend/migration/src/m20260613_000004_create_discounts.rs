//! 折扣表：独立、显式、可叠加。org 维度 FK 待 organizations 表落地后补。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Discounts::Table)
                    .if_not_exists()
                    .col(pk_auto(Discounts::Id))
                    .col(string(Discounts::Name))
                    .col(string(Discounts::Scope))
                    .col(integer_null(Discounts::TargetOrgId))
                    .col(integer_null(Discounts::TargetGroupId))
                    .col(integer_null(Discounts::TargetModelId))
                    .col(string(Discounts::Kind))
                    // 16,4：percentage（0.9000）足够，fixed 大额企业减免也不溢出
                    .col(decimal_len(Discounts::Value, 16, 4))
                    .col(boolean(Discounts::Stackable).default(false))
                    .col(integer(Discounts::Priority).default(0))
                    .col(timestamp_with_time_zone(Discounts::ValidFrom))
                    .col(timestamp_with_time_zone_null(Discounts::ValidTo))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_discounts_scope")
                    .table(Discounts::Table)
                    .col(Discounts::Scope)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Discounts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Discounts {
    Table,
    Id,
    Name,
    Scope,
    TargetOrgId,
    TargetGroupId,
    TargetModelId,
    Kind,
    Value,
    Stackable,
    Priority,
    ValidFrom,
    ValidTo,
}
