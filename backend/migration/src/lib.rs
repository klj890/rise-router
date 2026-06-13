//! SeaORM 迁移集。按 docs/data-model.md 十大域逐表补充。

pub use sea_orm_migration::prelude::*;

mod m20260613_000001_create_groups;
mod m20260613_000002_create_models;
mod m20260613_000003_create_prices;
mod m20260613_000004_create_discounts;
mod m20260613_000005_create_channels;
mod m20260613_000006_create_model_channels;

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
        ]
    }
}
