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
    /// Redis 连接池（多模态任务队列等用）。M0 容忍缺省；池创建惰性，不在启动时连接。
    pub redis: Option<deadpool_redis::Pool>,
}

impl AppState {
    pub fn new(config: Config, db: Option<DatabaseConnection>) -> Self {
        Self {
            config: Arc::new(config),
            db,
            redis: None,
        }
    }

    /// 注入 Redis 池（链式，保持 `new` 签名不变）。
    pub fn with_redis(mut self, pool: deadpool_redis::Pool) -> Self {
        self.redis = Some(pool);
        self
    }

    /// 取数据库连接；未连接时返回 [`crate::AppError::Internal`]。
    pub fn db(&self) -> crate::AppResult<&DatabaseConnection> {
        self.db
            .as_ref()
            .ok_or_else(|| crate::AppError::Internal("database not connected".into()))
    }

    /// 取 Redis 池；未配置时返回 [`crate::AppError::Internal`]。
    pub fn redis(&self) -> crate::AppResult<&deadpool_redis::Pool> {
        self.redis
            .as_ref()
            .ok_or_else(|| crate::AppError::Internal("redis not configured".into()))
    }
}
