//! 调用计费流水（只追加，对应 new-api logs）。
//!
//! 设计要点（docs/data-model.md §6）：
//! - **只追加审计表**：无 updated_at/deleted_at，不加外键（软引用 org/api_key/model/channel，
//!   避免删渠道/分组时连带删历史账，也省去高频写的 FK 校验开销）；靠索引保证查询。
//! - **唯一会无界增长的表** → PK 用 bigint（其余维度表保持 i32 serial，互不影响）。
//! - `group_slug` 是**计费当下的分组快照**：事后改 org 分组不影响历史账。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UsageLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UsageLogs::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(integer(UsageLogs::OrgId))
                    .col(integer_null(UsageLogs::UserId))
                    .col(integer(UsageLogs::ApiKeyId))
                    .col(integer_null(UsageLogs::AppId))
                    .col(integer(UsageLogs::ModelId))
                    .col(integer(UsageLogs::ChannelId))
                    // 计费快照分组（org 无分组时为空 = 默认价）
                    .col(string_len_null(UsageLogs::GroupSlug, 64))
                    .col(string_len_null(UsageLogs::RequestId, 128))
                    .col(string_len(UsageLogs::BillingUnit, 16))
                    // 用量 {input,output} / {seconds,resolution} 等
                    .col(json_binary(UsageLogs::Quantity))
                    // 18,8：6 位会把极便宜模型的微小调用 round 到 0（高频免费洞），故用 8 位
                    .col(decimal_len(UsageLogs::BaseAmount, 18, 8))
                    .col(json_binary_null(UsageLogs::DiscountDetail))
                    .col(decimal_len(UsageLogs::ChargedAmount, 18, 8))
                    // 渠道成本（毛利报表用）；渠道成本字段未建，暂留空
                    .col(decimal_len_null(UsageLogs::CostAmount, 18, 8))
                    .col(integer_null(UsageLogs::LatencyMs))
                    .col(boolean(UsageLogs::IsStream).default(false))
                    .col(
                        timestamp_with_time_zone(UsageLogs::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        // 流水查询热路径：按 org / 时间倒序（客户/财务看流水）
        manager
            .create_index(
                Index::create()
                    .name("idx_usage_logs_org_created")
                    .table(UsageLogs::Table)
                    .col(UsageLogs::OrgId)
                    .col(UsageLogs::CreatedAt)
                    .to_owned(),
            )
            .await?;
        // 按密钥看流水
        manager
            .create_index(
                Index::create()
                    .name("idx_usage_logs_key_created")
                    .table(UsageLogs::Table)
                    .col(UsageLogs::ApiKeyId)
                    .col(UsageLogs::CreatedAt)
                    .to_owned(),
            )
            .await?;
        // 全局时间维度（对账/报表跨 org 聚合）
        manager
            .create_index(
                Index::create()
                    .name("idx_usage_logs_created")
                    .table(UsageLogs::Table)
                    .col(UsageLogs::CreatedAt)
                    .to_owned(),
            )
            .await?;
        // 看流水游标分页支撑：org 内按 id 倒序定位（WHERE org_id=? AND id<? ORDER BY id DESC）
        manager
            .create_index(
                Index::create()
                    .name("idx_usage_logs_org_id")
                    .table(UsageLogs::Table)
                    .col(UsageLogs::OrgId)
                    .col(UsageLogs::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UsageLogs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum UsageLogs {
    Table,
    Id,
    OrgId,
    UserId,
    ApiKeyId,
    AppId,
    ModelId,
    ChannelId,
    GroupSlug,
    RequestId,
    BillingUnit,
    Quantity,
    BaseAmount,
    DiscountDetail,
    ChargedAmount,
    CostAmount,
    LatencyMs,
    IsStream,
    CreatedAt,
}
