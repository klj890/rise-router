//! 渠道健康管理字段：连通性测试 / 测速 / 自动禁用所需的列。
//!
//! 复用既有 `ChannelStatus`（Enabled/Disabled/CircuitBroken）语义：手动 `Disabled` 与自动
//! `CircuitBroken` 分开，被动恢复只动 CircuitBroken。本迁移只加列（独立 ALTER，保持既有迁移不可变）。
//! - response_time / test_time：最近一次测速结果与时间（前端展示渠道健康）。
//! - test_model：渠道测试默认模型（空时由 test 端点取该渠道 model_channels 首条）。
//! - auto_ban：渠道级是否允许被自动禁用（默认 true）。
//! - disabled_reason：自动禁用原因（便于排查）。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000005_create_channels::Channels;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Channels::Table)
                    .add_column(integer_null(ChannelsHealth::ResponseTime))
                    .add_column(timestamp_with_time_zone_null(ChannelsHealth::TestTime))
                    .add_column(string_null(ChannelsHealth::TestModel))
                    .add_column(boolean(ChannelsHealth::AutoBan).default(true))
                    .add_column(string_null(ChannelsHealth::DisabledReason))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Channels::Table)
                    .drop_column(ChannelsHealth::ResponseTime)
                    .drop_column(ChannelsHealth::TestTime)
                    .drop_column(ChannelsHealth::TestModel)
                    .drop_column(ChannelsHealth::AutoBan)
                    .drop_column(ChannelsHealth::DisabledReason)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ChannelsHealth {
    ResponseTime,
    TestTime,
    TestModel,
    AutoBan,
    DisabledReason,
}
