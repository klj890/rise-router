//! 把 api_keys 预算列精度从 numeric(18,6) 提到 numeric(18,8)，与 usage_logs 金额对齐。
//!
//! 起因（Gemini round-3）：6 位小数会把极便宜模型的微小调用（如 0.1 元/百万 × 3 token
//! = 0.0000003）round 到 0，高频微调用可无限免费。budget_used 累加 charged_amount，
//! 故须同精度，否则累加端再次损精度。api_keys 已在 000008 落地（已合并），用独立 ALTER 迁移
//! 而非改既有迁移（保持迁移不可变）。
use sea_orm_migration::{prelude::*, schema::*};

use crate::m20260613_000008_create_api_keys::ApiKeys;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ApiKeys::Table)
                    .modify_column(decimal_len(ApiKeys::BudgetUsed, 18, 8).default(0))
                    .modify_column(decimal_len_null(ApiKeys::BudgetLimit, 18, 8))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ApiKeys::Table)
                    .modify_column(decimal_len(ApiKeys::BudgetUsed, 18, 6).default(0))
                    .modify_column(decimal_len_null(ApiKeys::BudgetLimit, 18, 6))
                    .to_owned(),
            )
            .await
    }
}
