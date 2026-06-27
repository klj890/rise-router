//! 多模态异步任务（M5a）：统一 `/v1/tasks` 的任务状态机。
//!
//! 设计要点（docs/roadmap.md §4 M5a）：
//! - 状态机 status：1 Queued / 2 Running / 3 Succeeded / 4 Failed / 5 Cancelled。
//! - **可恢复**：`vendor_task_id` 在提交上游后写入；worker 重启后 poller 凭它续 poll，不丢长视频任务。
//! - 计费快照：`model_slug`/`group_slug` 落创建当下值（事后改分组/模型不影响历史账，对齐 usage_logs）。
//! - 鉴权上下文软引用（org/api_key/model/channel，不加 FK，与 usage_logs 同策略）。
//! - 无界增长表 → PK bigint。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Tasks::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Tasks::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(integer(Tasks::OrgId))
                    .col(integer(Tasks::ApiKeyId))
                    .col(integer_null(Tasks::UserId))
                    .col(integer_null(Tasks::AppId))
                    // 任务类型：video.generation / image.generation / audio.speech …
                    .col(string_len(Tasks::Type, 48))
                    .col(integer(Tasks::ModelId))
                    .col(string_len(Tasks::ModelSlug, 128))
                    // 派发时解析的路由渠道；未派发前为空
                    .col(integer_null(Tasks::ChannelId))
                    // 计费快照分组（org 无分组时空 = 默认价）
                    .col(string_len_null(Tasks::GroupSlug, 64))
                    .col(small_integer(Tasks::Status).default(1))
                    // 标准输入字段（prompt 等）+ 厂商独有参数透传
                    .col(json_binary(Tasks::Input))
                    .col(json_binary_null(Tasks::Extra))
                    // 上游任务 id（提交后写入，poller 凭此续查 → 可恢复）
                    .col(string_len_null(Tasks::VendorTaskId, 128))
                    // 计费量纲数量（如 {seconds:5} / {images:4}）+ 折前/折后金额
                    .col(json_binary_null(Tasks::Usage))
                    .col(decimal_len_null(Tasks::BaseAmount, 18, 8))
                    .col(decimal_len_null(Tasks::ChargedAmount, 18, 8))
                    .col(text_null(Tasks::Error))
                    // webhook URL 常带 token/query，易超 varchar(255) → text
                    .col(text_null(Tasks::WebhookUrl))
                    // 回调投递状态：pending / delivered / failed
                    .col(string_len_null(Tasks::WebhookState, 16))
                    .col(string_len_null(Tasks::RequestId, 128))
                    .col(integer(Tasks::PollCount).default(0))
                    .col(
                        timestamp_with_time_zone(Tasks::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .col(timestamp_with_time_zone_null(Tasks::StartedAt))
                    .col(timestamp_with_time_zone_null(Tasks::FinishedAt))
                    .col(
                        timestamp_with_time_zone(Tasks::UpdatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        // 客户/前端列表：org 内按 id 倒序游标分页
        manager
            .create_index(
                Index::create()
                    .name("idx_tasks_org_id")
                    .table(Tasks::Table)
                    .col(Tasks::OrgId)
                    .col(Tasks::Id)
                    .to_owned(),
            )
            .await?;
        // poller 扫描：按 status 取 Running 任务
        manager
            .create_index(
                Index::create()
                    .name("idx_tasks_status")
                    .table(Tasks::Table)
                    .col(Tasks::Status)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Tasks::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Tasks {
    Table,
    Id,
    OrgId,
    ApiKeyId,
    UserId,
    AppId,
    Type,
    ModelId,
    ModelSlug,
    ChannelId,
    GroupSlug,
    Status,
    Input,
    Extra,
    VendorTaskId,
    Usage,
    BaseAmount,
    ChargedAmount,
    Error,
    WebhookUrl,
    WebhookState,
    RequestId,
    PollCount,
    CreatedAt,
    StartedAt,
    FinishedAt,
    UpdatedAt,
}
