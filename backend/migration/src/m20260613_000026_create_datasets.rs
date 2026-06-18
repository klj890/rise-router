//! 报表数据集表（M4 片A）。策展语义层：slug 唯一；source 指向代码白名单视图；
//! metrics/dimensions/rls_rule 为 JSONB（管理员策展 + 行级规则）。无外键（source 由代码校验）。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Datasets::Table)
                    .if_not_exists()
                    .col(pk_auto(Datasets::Id))
                    .col(string(Datasets::Slug))
                    .col(string(Datasets::Name))
                    .col(string(Datasets::Source))
                    .col(json_binary(Datasets::Metrics))
                    .col(json_binary(Datasets::Dimensions))
                    .col(json_binary(Datasets::RlsRule))
                    .col(string(Datasets::RequiredPermission))
                    .col(
                        timestamp_with_time_zone(Datasets::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_datasets_slug")
                    .table(Datasets::Table)
                    .col(Datasets::Slug)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Datasets::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Datasets {
    Table,
    Id,
    Slug,
    Name,
    Source,
    Metrics,
    Dimensions,
    RlsRule,
    RequiredPermission,
    CreatedAt,
}
