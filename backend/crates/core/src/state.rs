use crate::config::Config;
use object_store::aws::AmazonS3;
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
    /// S3 兼容对象存储（多模态任务产物）。S3-compat 后端（MinIO/OSS/COS）统一走 AmazonS3。
    pub store: Option<Arc<AmazonS3>>,
}

impl AppState {
    pub fn new(config: Config, db: Option<DatabaseConnection>) -> Self {
        Self {
            config: Arc::new(config),
            db,
            redis: None,
            store: None,
        }
    }

    /// 注入 Redis 池（链式，保持 `new` 签名不变）。
    pub fn with_redis(mut self, pool: deadpool_redis::Pool) -> Self {
        self.redis = Some(pool);
        self
    }

    /// 注入对象存储客户端（链式）。
    pub fn with_store(mut self, store: Arc<AmazonS3>) -> Self {
        self.store = Some(store);
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

    /// 取对象存储；未配置时返回 [`crate::AppError::Internal`]。
    pub fn store(&self) -> crate::AppResult<&Arc<AmazonS3>> {
        self.store
            .as_ref()
            .ok_or_else(|| crate::AppError::Internal("object store not configured".into()))
    }

    /// 为对象生成 presigned GET URL（TTL 取配置）。供任务产物临时下载。
    pub async fn presign_get(&self, key: &str) -> crate::AppResult<String> {
        use object_store::{path::Path, signer::Signer};
        let store = self.store()?;
        let ttl = std::time::Duration::from_secs(self.config.s3.presign_ttl_secs);
        let url = store
            .signed_url(axum::http::Method::GET, &Path::from(key), ttl)
            .await
            .map_err(|e| crate::AppError::Internal(format!("presign failed: {e}")))?;
        Ok(url.to_string())
    }
}
