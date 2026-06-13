//! 初始迁移：用户分组表（定价五要素之一·纯分类，不含任何价格字段）。
//! 选它作首条迁移，因为它无外键、最简单，足以验证迁移框架可建可回滚。

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Groups::Table)
                    .if_not_exists()
                    .col(pk_auto(Groups::Id))
                    .col(string_uniq(Groups::Slug))
                    .col(string(Groups::Name))
                    .col(text_null(Groups::Description))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Groups::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Groups {
    Table,
    Id,
    Slug,
    Name,
    Description,
}
