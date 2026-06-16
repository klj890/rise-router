//! CRM 客户跟进记录（M3 片 A）。销售/客服对某客户（组织）记录跟进内容。
//! org 必填连带删（cascade）：组织注销则其跟进记录一并清理（非财务凭证，无独立留存义务）。
//! author_id 软引用记录人（users.id，**不建 FK**，可空）——用户注销不应连带删历史跟进，
//! 超管令牌（无用户上下文）创建时为空，仅留 id 作追溯。
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
                    .table(CustomerNotes::Table)
                    .if_not_exists()
                    .col(pk_auto(CustomerNotes::Id))
                    .col(integer(CustomerNotes::OrgId))
                    // 记录人（users.id 软引用，不建 FK；超管令牌创建时为空）
                    .col(integer_null(CustomerNotes::AuthorId))
                    .col(text(CustomerNotes::Content))
                    .col(
                        timestamp_with_time_zone(CustomerNotes::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_customer_notes_org")
                            .from(CustomerNotes::Table, CustomerNotes::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // org 内倒序游标分页（org_id + id）
        manager
            .create_index(
                Index::create()
                    .name("idx_customer_notes_org_id")
                    .table(CustomerNotes::Table)
                    .col(CustomerNotes::OrgId)
                    .col(CustomerNotes::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CustomerNotes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum CustomerNotes {
    Table,
    Id,
    OrgId,
    AuthorId,
    Content,
    CreatedAt,
}
