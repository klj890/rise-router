//! 组织（账户与计费主体）。商业分组挂这里；个人注册=org-of-one。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000001_create_groups::Groups;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Organizations::Table)
                    .if_not_exists()
                    .col(pk_auto(Organizations::Id))
                    .col(string(Organizations::Name))
                    .col(small_integer(Organizations::OrgType).default(1))
                    .col(integer_null(Organizations::GroupId))
                    .col(small_integer(Organizations::Status).default(1))
                    .col(small_integer(Organizations::RealnameStatus).default(0))
                    .col(integer_null(Organizations::OwnerSalesId))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_organizations_group")
                            .from(Organizations::Table, Organizations::GroupId)
                            .to(Groups::Table, Groups::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_organizations_group_id")
                    .table(Organizations::Table)
                    .col(Organizations::GroupId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Organizations::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Organizations {
    Table,
    Id,
    Name,
    OrgType,
    GroupId,
    Status,
    RealnameStatus,
    OwnerSalesId,
}
