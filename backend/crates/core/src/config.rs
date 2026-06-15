use std::env;

/// SMTP 邮件发送配置（`RR_SMTP_*`）。`host` 或 `from` 缺失 → 整体视为未配置（邮件功能禁用）。
#[derive(Clone, Debug)]
pub struct SmtpConfig {
    pub host: String,
    /// 465=隐式 TLS（relay）/ 587=STARTTLS / 其他=明文（仅测试）
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    /// 发件人地址（形如 `Rise Router <noreply@example.com>` 或纯邮箱）
    pub from: String,
}

/// 月度毛利月报邮件配置（`RR_BILLING_EMAIL_*`）。运维侧部署配置，非业务数据。
#[derive(Clone, Debug)]
pub struct BillingEmailConfig {
    /// 总开关：true 才启动 cron（默认 false）
    pub enabled: bool,
    /// 收件人列表（逗号/分号分隔，已去空白与空项）
    pub recipients: Vec<String>,
    /// 每月触发日（1-28，默认 1）
    pub day: u32,
    /// 触发小时（0-23，CST/UTC+8，默认 9）
    pub hour: u32,
    /// 演练模式：true 时只渲染 + 日志，不真发 SMTP（本地/无 SMTP 验证用）
    pub dry_run: bool,
}

/// 运行期配置。M0 阶段从环境变量加载（配套 `.env.example`）；
/// 后续里程碑如需分层配置文件再行扩展。
#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub redis_url: String,
    pub log_level: String,
    /// 管理操作令牌（如手动充值）。RBAC 落地前的临时守卫；未设则相关管理端点禁用。
    pub admin_token: Option<String>,
    /// 用户会话 JWT 签名密钥（手机号+短信登录签发 token）。未设则用户登录端点禁用（503）。
    pub jwt_secret: Option<String>,
    /// 引导管理员手机号：该号登录时自动授予 admin 角色（解决 RBAC 首个 admin 的鸡生蛋）。
    pub bootstrap_admin_phone: Option<String>,
    /// SMTP 配置；None = 未配置（邮件功能禁用）。
    pub smtp: Option<SmtpConfig>,
    /// 月度毛利月报邮件 cron 配置。
    pub billing_email: BillingEmailConfig,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind_addr: env::var("RR_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8088".into()),
            database_url: env::var("RR_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://rise:rise@localhost:5432/rise_router".into()),
            redis_url: env::var("RR_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into()),
            log_level: env::var("RR_LOG_LEVEL").unwrap_or_else(|_| "info".into()),
            admin_token: env::var("RR_ADMIN_TOKEN").ok().filter(|s| !s.is_empty()),
            jwt_secret: env::var("RR_JWT_SECRET").ok().filter(|s| !s.is_empty()),
            bootstrap_admin_phone: env::var("RR_BOOTSTRAP_ADMIN_PHONE")
                .ok()
                .filter(|s| !s.is_empty()),
            smtp: Self::smtp_from_env(),
            billing_email: BillingEmailConfig {
                enabled: env_bool("RR_BILLING_EMAIL_ENABLED"),
                recipients: env::var("RR_BILLING_EMAIL_RECIPIENTS")
                    .unwrap_or_default()
                    .split([',', ';'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                day: env_u32("RR_BILLING_EMAIL_DAY", 1),
                hour: env_u32("RR_BILLING_EMAIL_HOUR", 9),
                dry_run: env_bool("RR_BILLING_EMAIL_DRY_RUN"),
            },
        }
    }

    /// SMTP：host 与 from 同时存在才算配置完整，否则 None（邮件禁用）。
    fn smtp_from_env() -> Option<SmtpConfig> {
        let host = env::var("RR_SMTP_HOST").ok().filter(|s| !s.is_empty())?;
        let from = env::var("RR_SMTP_FROM").ok().filter(|s| !s.is_empty())?;
        Some(SmtpConfig {
            host,
            port: env_u32("RR_SMTP_PORT", 465) as u16,
            user: env::var("RR_SMTP_USER").ok().filter(|s| !s.is_empty()),
            password: env::var("RR_SMTP_PASSWORD").ok().filter(|s| !s.is_empty()),
            from,
        })
    }
}

/// 读布尔环境变量：`true`/`1`（忽略大小写）为真，其余/缺失为假。
fn env_bool(key: &str) -> bool {
    env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

/// 读 u32 环境变量；缺失或非法回退到 `default`。
fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
