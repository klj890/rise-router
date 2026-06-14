//! 应收侧对账单（M2 片 C）。按周期（月）聚合 usage_logs 营收 + 调用数，draft→locked 封账。
//! 每周期唯一一张（period 唯一约束）。upstream_cost/gap 预留毛利字段，渠道成本字段未建前留 NULL。
//! detail 为模型级明细 jsonb 数组。锁定后只读（封账）。
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Reconciliations::Table)
                    .if_not_exists()
                    .col(pk_auto(Reconciliations::Id))
                    // 对账周期，形如 2026-06（YYYY-MM）
                    .col(string_len(Reconciliations::Period, 16))
                    // 1=draft 2=locked
                    .col(small_integer(Reconciliations::Status).default(1))
                    // 应收：SUM(usage_logs.charged_amount)
                    .col(decimal_len(Reconciliations::TotalRevenue, 18, 8))
                    // 调用数：COUNT(*)
                    .col(big_integer(Reconciliations::TotalCalls))
                    // 渠道成本（毛利报表用）；渠道成本字段未建前留 NULL
                    .col(decimal_len_null(Reconciliations::UpstreamCost, 18, 8))
                    // 毛利缺口 = 应收 − 成本；成本未建前留 NULL
                    .col(decimal_len_null(Reconciliations::Gap, 18, 8))
                    // 模型级明细 [{model_id, revenue, calls}]
                    .col(json_binary_null(Reconciliations::Detail))
                    .col(
                        timestamp_with_time_zone(Reconciliations::GeneratedAt)
                            .default(Expr::current_timestamp()),
                    )
                    // 封账时间（locked 时回填）
                    .col(timestamp_with_time_zone_null(Reconciliations::LockedAt))
                    .to_owned(),
            )
            .await?;
        // 每周期唯一一张对账单
        manager
            .create_index(
                Index::create()
                    .name("uq_reconciliations_period")
                    .table(Reconciliations::Table)
                    .col(Reconciliations::Period)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Reconciliations::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Reconciliations {
    Table,
    Id,
    Period,
    Status,
    TotalRevenue,
    TotalCalls,
    UpstreamCost,
    Gap,
    Detail,
    GeneratedAt,
    LockedAt,
}
