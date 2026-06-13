//! 路由表（model↔channel）：能力可达 + 负载，剥离 group/价格。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000002_create_models::Models;
use crate::m20260613_000005_create_channels::Channels;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ModelChannels::Table)
                    .if_not_exists()
                    .col(pk_auto(ModelChannels::Id))
                    .col(integer(ModelChannels::ModelId))
                    .col(integer(ModelChannels::ChannelId))
                    .col(string(ModelChannels::UpstreamModelName))
                    .col(boolean(ModelChannels::Enabled).default(true))
                    .col(integer_null(ModelChannels::Priority))
                    .col(integer_null(ModelChannels::Weight))
                    .col(json_binary_null(ModelChannels::CostPrice))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_model_channels_model")
                            .from(ModelChannels::Table, ModelChannels::ModelId)
                            .to(Models::Table, Models::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_model_channels_channel")
                            .from(ModelChannels::Table, ModelChannels::ChannelId)
                            .to(Channels::Table, Channels::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // 同一 (model, channel) 唯一
        manager
            .create_index(
                Index::create()
                    .name("uq_model_channels")
                    .table(ModelChannels::Table)
                    .col(ModelChannels::ModelId)
                    .col(ModelChannels::ChannelId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        // 路由查询热路径
        manager
            .create_index(
                Index::create()
                    .name("idx_model_channels_route")
                    .table(ModelChannels::Table)
                    .col(ModelChannels::ModelId)
                    .col(ModelChannels::Enabled)
                    .to_owned(),
            )
            .await?;
        // FK ChannelId 索引：PG 不为外键自动建索引，优化级联删除避免全表扫描
        manager
            .create_index(
                Index::create()
                    .name("idx_model_channels_channel_id")
                    .table(ModelChannels::Table)
                    .col(ModelChannels::ChannelId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ModelChannels::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ModelChannels {
    Table,
    Id,
    ModelId,
    ChannelId,
    UpstreamModelName,
    Enabled,
    Priority,
    Weight,
    CostPrice,
}
