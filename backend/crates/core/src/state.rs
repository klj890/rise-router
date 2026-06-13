use crate::config::Config;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

/// 共享应用状态，注入到所有 axum handler。
///
/// `db` 为 `Option`：M0 允许在无数据库时启动（脚手架可空跑），
/// 连接成功后各域 handler 通过 `state.db()` 取用。
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Option<DatabaseConnection>,
}

impl AppState {
    pub fn new(config: Config, db: Option<DatabaseConnection>) -> Self {
        Self {
            config: Arc::new(config),
            db,
        }
    }

    /// 取数据库连接；未连接时返回 [`crate::AppError::Internal`]。
    pub fn db(&self) -> crate::AppResult<&DatabaseConnection> {
        self.db
            .as_ref()
            .ok_or_else(|| crate::AppError::Internal("database not connected".into()))
    }
}
