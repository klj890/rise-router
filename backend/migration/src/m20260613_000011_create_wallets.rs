//! 账户钱包（挂组织，一对一）。余额=真实的钱；授信=企业后付费额度；冻结=预扣占用。
//! 可用额度 = balance + credit_limit − frozen。金额精度对齐 numeric(18,8)。
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
                    .table(Wallets::Table)
                    .if_not_exists()
                    .col(pk_auto(Wallets::Id))
                    .col(integer_uniq(Wallets::OrgId))
                    .col(decimal_len(Wallets::Balance, 18, 8).default(0))
                    .col(decimal_len(Wallets::CreditLimit, 18, 8).default(0))
                    .col(decimal_len(Wallets::Frozen, 18, 8).default(0))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_wallets_org")
                            .from(Wallets::Table, Wallets::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Wallets::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Wallets {
    Table,
    Id,
    OrgId,
    Balance,
    CreditLimit,
    Frozen,
}
