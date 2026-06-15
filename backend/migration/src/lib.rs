//! SeaORM 迁移集。按 docs/data-model.md 十大域逐表补充。

pub use sea_orm_migration::prelude::*;

mod m20260613_000001_create_groups;
mod m20260613_000002_create_models;
mod m20260613_000003_create_prices;
mod m20260613_000004_create_discounts;
mod m20260613_000005_create_channels;
mod m20260613_000006_create_model_channels;
mod m20260613_000007_create_organizations;
mod m20260613_000008_create_api_keys;
mod m20260613_000009_create_usage_logs;
mod m20260613_000010_widen_budget_precision;
mod m20260613_000011_create_wallets;
mod m20260613_000012_create_transactions;
mod m20260613_000013_create_orders;
mod m20260613_000014_create_reconciliations;
mod m20260613_000015_create_users;
mod m20260613_000016_create_phone_codes;
mod m20260613_000017_create_roles;
mod m20260613_000018_create_permissions;
mod m20260613_000019_create_role_permissions;
mod m20260613_000020_create_user_roles;
mod m20260613_000021_add_phone_code_attempts;
mod m20260613_000022_create_invoices;
mod m20260613_000023_create_cron_state;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260613_000001_create_groups::Migration),
            Box::new(m20260613_000002_create_models::Migration),
            Box::new(m20260613_000003_create_prices::Migration),
            Box::new(m20260613_000004_create_discounts::Migration),
            Box::new(m20260613_000005_create_channels::Migration),
            Box::new(m20260613_000006_create_model_channels::Migration),
            Box::new(m20260613_000007_create_organizations::Migration),
            Box::new(m20260613_000008_create_api_keys::Migration),
            Box::new(m20260613_000009_create_usage_logs::Migration),
            Box::new(m20260613_000010_widen_budget_precision::Migration),
            Box::new(m20260613_000011_create_wallets::Migration),
            Box::new(m20260613_000012_create_transactions::Migration),
            Box::new(m20260613_000013_create_orders::Migration),
            Box::new(m20260613_000014_create_reconciliations::Migration),
            Box::new(m20260613_000015_create_users::Migration),
            Box::new(m20260613_000016_create_phone_codes::Migration),
            Box::new(m20260613_000017_create_roles::Migration),
            Box::new(m20260613_000018_create_permissions::Migration),
            Box::new(m20260613_000019_create_role_permissions::Migration),
            Box::new(m20260613_000020_create_user_roles::Migration),
            Box::new(m20260613_000021_add_phone_code_attempts::Migration),
            Box::new(m20260613_000022_create_invoices::Migration),
            Box::new(m20260613_000023_create_cron_state::Migration),
        ]
    }
}
