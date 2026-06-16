//! 后台任务状态 KV（M2 片F·Part2）。极简 key/value 表，仅供 cron 记录防重状态
//! （如 `billing.monthly.last_sent` = 上次月报发送的 unix 时间戳），进程重启不重发。
//!
//! **刻意不做通用 settings 基建**：SMTP/收件人/开关等运维配置走环境变量（`RR_SMTP_*` /
//! `RR_BILLING_EMAIL_*`）；本表只承载"任务执行到哪了"这类运行时状态，不暴露 CRUD。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CronState::Table)
                    .if_not_exists()
                    .col(string(CronState::Key).primary_key())
                    .col(string(CronState::Value))
                    .col(
                        timestamp_with_time_zone(CronState::UpdatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CronState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum CronState {
    Table,
    Key,
    Value,
    UpdatedAt,
}
