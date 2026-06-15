//! 用户（登录主体，组织成员）。手机号为国情主注册通道（唯一）。
//! password_hash 可空（手机号+短信为主通道，密码可选）；微信等第三方登录走旁表（后续）。
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
                    .table(Users::Table)
                    .if_not_exists()
                    .col(pk_auto(Users::Id))
                    .col(integer(Users::OrgId))
                    .col(string_uniq(Users::Phone))
                    .col(string_null(Users::Email))
                    .col(string_null(Users::PasswordHash))
                    .col(string_null(Users::Nickname))
                    .col(small_integer(Users::Status).default(1))
                    .col(timestamp_with_time_zone_null(Users::LastLoginAt))
                    .col(timestamp_with_time_zone(Users::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_users_org")
                            .from(Users::Table, Users::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_users_org_id")
                    .table(Users::Table)
                    .col(Users::OrgId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Users {
    Table,
    Id,
    OrgId,
    Phone,
    Email,
    PasswordHash,
    Nickname,
    Status,
    LastLoginAt,
    CreatedAt,
}
