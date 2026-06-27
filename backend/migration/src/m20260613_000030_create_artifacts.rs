//! 任务产物（M5a）：异步任务完成后落对象存储的工件元数据。
//!
//! 实际字节落 S3 兼容存储（MinIO/OSS/COS）；本表只存元数据 + `s3_key`，
//! 对外通过 presigned URL 临时下载。task_id 软引用（不加 FK，与流水表同策略）。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Artifacts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Artifacts::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(big_integer(Artifacts::TaskId))
                    .col(string(Artifacts::Bucket))
                    .col(string(Artifacts::S3Key))
                    .col(string_len(Artifacts::ContentType, 128))
                    .col(big_integer_null(Artifacts::SizeBytes))
                    // 宽高/时长等：{width,height,duration_s}
                    .col(json_binary_null(Artifacts::Meta))
                    .col(
                        timestamp_with_time_zone(Artifacts::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_artifacts_task")
                    .table(Artifacts::Table)
                    .col(Artifacts::TaskId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Artifacts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Artifacts {
    Table,
    Id,
    TaskId,
    Bucket,
    S3Key,
    ContentType,
    SizeBytes,
    Meta,
    CreatedAt,
}
