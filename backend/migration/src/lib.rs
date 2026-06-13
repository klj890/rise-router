//! SeaORM 迁移集。M0 仅含一条初始迁移（`groups` 表），验证 up/down 框架；
//! 后续里程碑按 `docs/data-model.md` 十大域逐表补充。

pub use sea_orm_migration::prelude::*;

mod m20260613_000001_create_groups;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260613_000001_create_groups::Migration)]
    }
}
