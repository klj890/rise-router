//! 定制报表定义表（M4 片A）。报表只能基于数据集（dataset_id 连带删）；
//! visibility 控共享范围；config 存指标/维度/过滤/图表类型。owner_user_id 软引用（超管令牌创建为空）。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000026_create_datasets::Datasets;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ReportDefinitions::Table)
                    .if_not_exists()
                    .col(pk_auto(ReportDefinitions::Id))
                    .col(integer(ReportDefinitions::DatasetId))
                    .col(string(ReportDefinitions::Name))
                    .col(integer_null(ReportDefinitions::OwnerUserId))
                    .col(string(ReportDefinitions::Visibility).default("private"))
                    .col(json_binary(ReportDefinitions::Config))
                    .col(
                        timestamp_with_time_zone(ReportDefinitions::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_report_definitions_dataset")
                            .from(ReportDefinitions::Table, ReportDefinitions::DatasetId)
                            .to(Datasets::Table, Datasets::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // 按 owner 倒序游标分页
        manager
            .create_index(
                Index::create()
                    .name("idx_report_definitions_owner")
                    .table(ReportDefinitions::Table)
                    .col(ReportDefinitions::OwnerUserId)
                    .col(ReportDefinitions::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ReportDefinitions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ReportDefinitions {
    Table,
    Id,
    DatasetId,
    Name,
    OwnerUserId,
    Visibility,
    Config,
    CreatedAt,
}
