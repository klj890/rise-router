//! 价格表：按 (模型 × 分组) 存显式单价；group_id 为空=该模型默认价。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000001_create_groups::Groups;
use crate::m20260613_000002_create_models::Models;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Prices::Table)
                    .if_not_exists()
                    .col(pk_auto(Prices::Id))
                    .col(integer(Prices::ModelId))
                    .col(integer_null(Prices::GroupId))
                    .col(string(Prices::BillingUnit))
                    .col(string(Prices::Currency).default("CNY"))
                    .col(json_binary(Prices::UnitPrices))
                    .col(timestamp_with_time_zone(Prices::ValidFrom))
                    .col(timestamp_with_time_zone_null(Prices::ValidTo))
                    .col(integer(Prices::Version).default(1))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_prices_model")
                            .from(Prices::Table, Prices::ModelId)
                            .to(Models::Table, Models::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_prices_group")
                            .from(Prices::Table, Prices::GroupId)
                            .to(Groups::Table, Groups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // 价格解析热路径索引
        manager
            .create_index(
                Index::create()
                    .name("idx_prices_lookup")
                    .table(Prices::Table)
                    .col(Prices::ModelId)
                    .col(Prices::GroupId)
                    .col(Prices::ValidFrom)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Prices::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
// UnitPrices 列名即 unit_prices，恰好以表名 Prices 结尾，非命名问题
#[allow(clippy::enum_variant_names)]
pub enum Prices {
    Table,
    Id,
    ModelId,
    GroupId,
    BillingUnit,
    Currency,
    UnitPrices,
    ValidFrom,
    ValidTo,
    Version,
}
