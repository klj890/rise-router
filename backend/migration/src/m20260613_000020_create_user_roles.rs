//! 用户↔角色（M:N，可带数据域 scope 供报表 RLS 取用）。(user_id, role_id) 唯一。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000015_create_users::Users;
use crate::m20260613_000017_create_roles::Roles;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserRoles::Table)
                    .if_not_exists()
                    .col(pk_auto(UserRoles::Id))
                    .col(integer(UserRoles::UserId))
                    .col(integer(UserRoles::RoleId))
                    .col(json_binary_null(UserRoles::Scope))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_roles_user")
                            .from(UserRoles::Table, UserRoles::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_roles_role")
                            .from(UserRoles::Table, UserRoles::RoleId)
                            .to(Roles::Table, Roles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_user_roles")
                    .table(UserRoles::Table)
                    .col(UserRoles::UserId)
                    .col(UserRoles::RoleId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserRoles::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum UserRoles {
    Table,
    Id,
    UserId,
    RoleId,
    Scope,
}
