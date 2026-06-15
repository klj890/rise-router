//! 给 phone_codes 加 attempts 计数：限制单个验证码的错误尝试次数（防暴力枚举）。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PhoneCodes::Table)
                    .add_column(small_integer(PhoneCodes::Attempts).default(0))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PhoneCodes::Table)
                    .drop_column(PhoneCodes::Attempts)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PhoneCodes {
    Table,
    Attempts,
}
