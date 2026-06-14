//! 资金流水（只追加）：充值/消费/退款/调整/授信还款。amount 有符号；balance_after 为快照。
//! 与 usage_logs 同为高写入只追加表，PK 用 bigint；软引用 org（不连带删历史账）。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Transactions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(integer(Transactions::OrgId))
                    // 1充值 2消费 3退款 4调整 5授信还款
                    .col(small_integer(Transactions::Kind))
                    .col(decimal_len(Transactions::Amount, 18, 8))
                    .col(decimal_len(Transactions::BalanceAfter, 18, 8))
                    // 关联来源：usage_log / order / manual 等
                    .col(string_len_null(Transactions::RefType, 32))
                    .col(big_integer_null(Transactions::RefId))
                    .col(string_len_null(Transactions::Memo, 256))
                    .col(
                        timestamp_with_time_zone(Transactions::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_transactions_org_id")
                    .table(Transactions::Table)
                    .col(Transactions::OrgId)
                    .col(Transactions::Id)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_transactions_created")
                    .table(Transactions::Table)
                    .col(Transactions::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Transactions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Transactions {
    Table,
    Id,
    OrgId,
    Kind,
    Amount,
    BalanceAfter,
    RefType,
    RefId,
    Memo,
    CreatedAt,
}
