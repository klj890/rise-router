//! 渠道（上游接入·纯接入，成本与售价分离）。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Channels::Table)
                    .if_not_exists()
                    .col(pk_auto(Channels::Id))
                    .col(string(Channels::Name))
                    .col(string(Channels::ProtocolAdapter))
                    .col(string(Channels::BaseUrl))
                    .col(json_binary(Channels::Credentials))
                    .col(json_binary_null(Channels::AdapterConfig))
                    .col(integer(Channels::Priority).default(0))
                    .col(integer(Channels::Weight).default(0))
                    .col(small_integer(Channels::Status).default(1))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Channels::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Channels {
    Table,
    Id,
    Name,
    ProtocolAdapter,
    BaseUrl,
    Credentials,
    AdapterConfig,
    Priority,
    Weight,
    Status,
}
