//! 模型能力目录（纯能力，无价格）。display_name 用 i18n JSONB。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Models::Table)
                    .if_not_exists()
                    .col(pk_auto(Models::Id))
                    .col(string_uniq(Models::Slug))
                    .col(json_binary(Models::DisplayNameI18n))
                    .col(string(Models::Modality))
                    .col(string(Models::Invocation))
                    .col(string(Models::BillingUnit))
                    .col(json_binary_null(Models::Capabilities))
                    .col(small_integer(Models::Status).default(1))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Models::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Models {
    Table,
    Id,
    Slug,
    DisplayNameI18n,
    Modality,
    Invocation,
    BillingUnit,
    Capabilities,
    Status,
}
