//! 为 usage_logs.request_id 建索引（M5a 片C）。
//!
//! 多模态任务结算用 `request_id = rr-task-{id}` 做幂等去重检查；usage_logs 无界增长，
//! 无索引则该等值查询全表扫描。非唯一索引（chat 的 request_id 可空/客户端提供可能重复，
//! 唯一约束有破坏风险；任务侧幂等由应用层 + poller 去重保证）。
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_usage_logs_request_id")
                    .table(UsageLogs::Table)
                    .col(UsageLogs::RequestId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_usage_logs_request_id")
                    .table(UsageLogs::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum UsageLogs {
    Table,
    RequestId,
}
