//! 迁移 CLI。读取 `DATABASE_URL` 环境变量。
//!
//! 用法：
//!   cargo run -p migration -- up        # 应用迁移
//!   cargo run -p migration -- down      # 回滚一步
//!   cargo run -p migration -- fresh     # 删表重建
//!   cargo run -p migration -- status    # 查看状态

#[tokio::main]
async fn main() {
    sea_orm_migration::cli::run_cli(migration::Migrator).await;
}
