use std::env;

/// 运行期配置。M0 阶段从环境变量加载（配套 `.env.example`）；
/// 后续里程碑如需分层配置文件再行扩展。
#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub redis_url: String,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind_addr: env::var("RR_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8088".into()),
            database_url: env::var("RR_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://rise:rise@localhost:5432/rise_router".into()),
            redis_url: env::var("RR_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into()),
            log_level: env::var("RR_LOG_LEVEL").unwrap_or_else(|_| "info".into()),
        }
    }
}
