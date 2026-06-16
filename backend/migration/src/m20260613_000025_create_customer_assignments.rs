//! CRM 客户归属变更历史（M3 片 A）。`organizations.owner_sales_id` 是当前归属**真相源**；
//! 本表记录归属的**变更轨迹**（业绩归因 / 审计）：每次改派关闭旧 active 行并插入新 active 行。
//! org 必填连带删（cascade）；sales_id 软引用销售（users.id，**不建 FK**）。
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
                    .table(CustomerAssignments::Table)
                    .if_not_exists()
                    .col(pk_auto(CustomerAssignments::Id))
                    .col(integer(CustomerAssignments::OrgId))
                    // 归属销售（users.id 软引用，不建 FK）
                    .col(integer(CustomerAssignments::SalesId))
                    .col(
                        timestamp_with_time_zone(CustomerAssignments::AssignedAt)
                            .default(Expr::current_timestamp()),
                    )
                    // 当前归属标记：改派时旧行置 false
                    .col(boolean(CustomerAssignments::Active).default(true))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_customer_assignments_org")
                            .from(CustomerAssignments::Table, CustomerAssignments::OrgId)
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
                    .name("idx_customer_assignments_org_id")
                    .table(CustomerAssignments::Table)
                    .col(CustomerAssignments::OrgId)
                    .col(CustomerAssignments::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CustomerAssignments::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum CustomerAssignments {
    Table,
    Id,
    OrgId,
    SalesId,
    AssignedAt,
    Active,
}
