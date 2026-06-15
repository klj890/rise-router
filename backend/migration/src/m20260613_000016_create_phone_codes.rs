//! 短信验证码（注册/登录主通道）。仅存验证码的 sha256 哈希，不存明文。
//! 限流：按 (phone, created_at) 索引查最近一条；过期/已用由 expires_at/consumed_at 控制。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PhoneCodes::Table)
                    .if_not_exists()
                    .col(pk_auto(PhoneCodes::Id))
                    .col(string(PhoneCodes::Phone))
                    .col(string(PhoneCodes::CodeHash))
                    .col(timestamp_with_time_zone(PhoneCodes::ExpiresAt))
                    .col(timestamp_with_time_zone_null(PhoneCodes::ConsumedAt))
                    .col(timestamp_with_time_zone(PhoneCodes::CreatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_phone_codes_phone_created")
                    .table(PhoneCodes::Table)
                    .col(PhoneCodes::Phone)
                    .col(PhoneCodes::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PhoneCodes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum PhoneCodes {
    Table,
    Id,
    Phone,
    CodeHash,
    ExpiresAt,
    ConsumedAt,
    CreatedAt,
}
