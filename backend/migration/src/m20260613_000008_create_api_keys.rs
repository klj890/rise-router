//! 虚拟密钥。仅存哈希；预算/模型白名单/过期挂 key。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000007_create_organizations::Organizations;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ApiKeys::Table)
                    .if_not_exists()
                    .col(pk_auto(ApiKeys::Id))
                    .col(integer(ApiKeys::OrgId))
                    .col(integer_null(ApiKeys::UserId))
                    .col(integer_null(ApiKeys::AppId))
                    .col(string_uniq(ApiKeys::KeyHash))
                    .col(string(ApiKeys::Name))
                    .col(json_binary_null(ApiKeys::AllowedModels))
                    .col(decimal_len_null(ApiKeys::BudgetLimit, 18, 6))
                    .col(decimal_len(ApiKeys::BudgetUsed, 18, 6).default(0))
                    .col(timestamp_with_time_zone_null(ApiKeys::ExpiresAt))
                    .col(small_integer(ApiKeys::Status).default(1))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_api_keys_org")
                            .from(ApiKeys::Table, ApiKeys::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_api_keys_org_id")
                    .table(ApiKeys::Table)
                    .col(ApiKeys::OrgId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ApiKeys::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ApiKeys {
    Table,
    Id,
    OrgId,
    UserId,
    AppId,
    KeyHash,
    Name,
    AllowedModels,
    BudgetLimit,
    BudgetUsed,
    ExpiresAt,
    Status,
}
