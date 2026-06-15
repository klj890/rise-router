//! 权限点（由 App 声明注入；内部模块也是 App，狗粮原则）。code 唯一，如 `pricing.manage`。
//! app_id FK 待 apps 表（M5）落地后补；当前用 `module` 软记来源模块。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Permissions::Table)
                    .if_not_exists()
                    .col(pk_auto(Permissions::Id))
                    .col(string_uniq(Permissions::Code))
                    .col(string_null(Permissions::Module))
                    .col(string_null(Permissions::Description))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Permissions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Permissions {
    Table,
    Id,
    Code,
    Module,
    Description,
}
