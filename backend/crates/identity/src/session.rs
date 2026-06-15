//! 用户会话：手机号 + 短信验证码注册/登录（国情主通道）+ JWT 会话令牌。
//!
//! 流程：`send-code`（生成 6 位码，sha256 入库，mock 下发）→ `login`（验码 → 无此手机号则
//! 事务建 org-of-one + user → 签发 JWT）→ `me`（Bearer JWT 回显用户+组织）。
//! 验证码仅存哈希、短时效 + 单次消费 + 60s 限流。JWT 与 api_key 鉴权两条独立路径：
//! `/me` 走用户 JWT，`/whoami` 走 api_key（互不接受对方令牌）。
//!
//! **mock 短信**：当前无真实短信网关，`send-code` 把验证码经 `dev_code` 字段回显并记日志；
//! 接入真实服务商时替换 [`deliver_sms`] 即可，不影响其余逻辑。

use axum::{extract::State, http::HeaderMap, Json};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use rise_core::{AppError, AppResult, AppState};
use rise_entity::{
    organizations::{self, OrgStatus, OrgType, RealnameStatus},
    phone_codes, users,
};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};

const CODE_TTL_SECS: i64 = 5 * 60;
const RESEND_COOLDOWN_SECS: i64 = 60;
const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;
/// 单个验证码允许的错误尝试次数，达到即作废（配合 60s 发码冷却，封死暴力枚举）。
const MAX_CODE_ATTEMPTS: i16 = 5;

/// JWT 载荷（用户会话）。sub=user_id，org=org_id。
#[derive(Serialize, Deserialize)]
pub struct UserClaims {
    pub sub: i32,
    pub org: i32,
    pub iat: usize,
    pub exp: usize,
}

/// 对外用户视图（**不含 password_hash**）。
#[derive(Serialize)]
pub struct UserView {
    id: i32,
    org_id: i32,
    phone: String,
    email: Option<String>,
    nickname: Option<String>,
    status: users::UserStatus,
    last_login_at: Option<DateTimeWithTimeZone>,
    created_at: DateTimeWithTimeZone,
}

impl From<users::Model> for UserView {
    fn from(m: users::Model) -> Self {
        Self {
            id: m.id,
            org_id: m.org_id,
            phone: m.phone,
            email: m.email,
            nickname: m.nickname,
            status: m.status,
            last_login_at: m.last_login_at,
            created_at: m.created_at,
        }
    }
}

/// 中国大陆手机号粗校验：11 位、首位 1、次位 3-9、全数字。
fn valid_phone(p: &str) -> bool {
    let b = p.as_bytes();
    p.len() == 11
        && b[0] == b'1'
        && (b'3'..=b'9').contains(&b[1])
        && p.chars().all(|c| c.is_ascii_digit())
}

/// 生成 6 位数字验证码。
fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

/// 验证码哈希：sha256(phone:code)，绑定手机号防跨号复用。
fn code_hash(phone: &str, code: &str) -> String {
    crate::hash_key(&format!("{phone}:{code}"))
}

/// mock 短信下发（接真实网关时替换此函数）。
fn deliver_sms(phone: &str, code: &str) {
    tracing::info!(
        phone,
        code,
        "mock SMS: 验证码下发（替换 deliver_sms 接真实网关）"
    );
}

fn jwt_secret(state: &AppState) -> AppResult<String> {
    state
        .config
        .jwt_secret
        .clone()
        .ok_or_else(|| AppError::Internal("auth not configured (set RR_JWT_SECRET)".into()))
}

#[derive(Deserialize)]
pub struct SendCodeReq {
    phone: String,
}

#[derive(Serialize)]
pub struct SendCodeResp {
    sent: bool,
    /// mock 网关回显：真实接入后移除
    dev_code: String,
}

/// `POST /api/identity/auth/send-code` —— 下发短信验证码（mock）。60s 限流。
pub async fn send_code(
    State(state): State<AppState>,
    Json(req): Json<SendCodeReq>,
) -> AppResult<Json<SendCodeResp>> {
    let db = state.db()?;
    let phone = req.phone.trim().to_owned();
    if !valid_phone(&phone) {
        return Err(AppError::BadRequest("invalid phone number".into()));
    }

    let now = chrono::Utc::now().fixed_offset();
    // 限流：最近一条码在冷却窗口内 → 429。
    if let Some(last) = phone_codes::Entity::find()
        .filter(phone_codes::Column::Phone.eq(&phone))
        .order_by_desc(phone_codes::Column::CreatedAt)
        .one(db)
        .await?
    {
        if (now - last.created_at).num_seconds() < RESEND_COOLDOWN_SECS {
            return Err(AppError::QuotaExceeded);
        }
    }

    let code = generate_code();
    let expires_at = now + chrono::Duration::seconds(CODE_TTL_SECS);
    phone_codes::ActiveModel {
        phone: Set(phone.clone()),
        code_hash: Set(code_hash(&phone, &code)),
        expires_at: Set(expires_at),
        consumed_at: Set(None),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await?;
    deliver_sms(&phone, &code);

    Ok(Json(SendCodeResp {
        sent: true,
        dev_code: code,
    }))
}

#[derive(Deserialize)]
pub struct LoginReq {
    phone: String,
    code: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    token: String,
    user: UserView,
    /// 是否本次新注册（前端可做引导）
    registered: bool,
}

/// `POST /api/identity/auth/login` —— 验码登录/注册：无此手机号则建 org-of-one + user，签发 JWT。
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> AppResult<Json<LoginResp>> {
    let secret = jwt_secret(&state)?;
    let db = state.db()?;
    let phone = req.phone.trim().to_owned();
    if !valid_phone(&phone) {
        return Err(AppError::BadRequest("invalid phone number".into()));
    }

    let now = chrono::Utc::now().fixed_offset();
    // 取最近一条未消费、未过期的验证码，比对哈希。
    let candidate = phone_codes::Entity::find()
        .filter(phone_codes::Column::Phone.eq(&phone))
        .filter(phone_codes::Column::ConsumedAt.is_null())
        .filter(phone_codes::Column::ExpiresAt.gt(now))
        .order_by_desc(phone_codes::Column::CreatedAt)
        .one(db)
        .await?
        .ok_or(AppError::BadRequest("code expired or not requested".into()))?;
    if candidate.code_hash != code_hash(&phone, req.code.trim()) {
        // 错误尝试 +1；达上限即作废该码（防暴力枚举：login 端点本身不限频，仅靠此计数 + 发码冷却封死）。
        let next_attempts = candidate.attempts + 1;
        let mut am = phone_codes::ActiveModel {
            id: Set(candidate.id),
            attempts: Set(next_attempts),
            ..Default::default()
        };
        if next_attempts >= MAX_CODE_ATTEMPTS {
            am.consumed_at = Set(Some(now));
        }
        am.update(db).await?;
        return Err(AppError::BadRequest("invalid code".into()));
    }

    // 验码通过。先查既有用户并校验状态——被停用用户**不消费**验证码（避免每次尝试白烧一个码）。
    let existing = users::find_by_phone(db, &phone).await?;
    if let Some(ref u) = existing {
        if u.status != users::UserStatus::Enabled {
            return Err(AppError::Forbidden);
        }
    }
    // 单次消费：标记 consumed（确认会继续登录/注册后才消费）。
    let mut consumed: phone_codes::ActiveModel = candidate.into();
    consumed.consumed_at = Set(Some(now));
    consumed.update(db).await?;

    // 注册或登录。
    let (user, registered) = match existing {
        Some(u) => {
            let mut am: users::ActiveModel = u.into();
            am.last_login_at = Set(Some(now));
            (am.update(db).await?, false)
        }
        None => {
            // 事务建 org-of-one + user，避免半成品孤儿组织。
            let p = phone.clone();
            let user = db
                .transaction::<_, users::Model, sea_orm::DbErr>(move |txn| {
                    Box::pin(async move {
                        let org = organizations::ActiveModel {
                            name: Set(format!("个人-{p}")),
                            org_type: Set(OrgType::Individual),
                            status: Set(OrgStatus::Active),
                            realname_status: Set(RealnameStatus::Unverified),
                            ..Default::default()
                        }
                        .insert(txn)
                        .await?;
                        users::ActiveModel {
                            org_id: Set(org.id),
                            phone: Set(p),
                            status: Set(users::UserStatus::Enabled),
                            last_login_at: Set(Some(now)),
                            created_at: Set(now),
                            ..Default::default()
                        }
                        .insert(txn)
                        .await
                    })
                })
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            (user, true)
        }
    };

    // 引导首个管理员：配置的引导手机号登录即自动授 admin（幂等）。失败仅告警不阻断登录。
    if state.config.bootstrap_admin_phone.as_deref() == Some(phone.as_str()) {
        if let Err(e) = rise_rbac::grant_role(db, user.id, "admin").await {
            tracing::warn!(error = %e, "bootstrap admin grant failed");
        }
    }

    let token = sign_token(&secret, user.id, user.org_id, now)?;
    Ok(Json(LoginResp {
        token,
        user: user.into(),
        registered,
    }))
}

#[derive(Serialize)]
pub struct MeResp {
    user: UserView,
    org: organizations::Model,
}

/// 从请求头取 Bearer 用户 JWT 并校验 → claims。供 [`crate::guard::require`] 与 `me` 复用。
/// 未配置 RR_JWT_SECRET → 503；无/非法 token → 401。
pub(crate) fn verify_request(state: &AppState, headers: &HeaderMap) -> AppResult<UserClaims> {
    let secret = jwt_secret(state)?;
    let raw = crate::bearer_token(headers).ok_or(AppError::Unauthorized)?;
    verify_token(&secret, raw)
}

/// `GET /api/identity/me`（Bearer 用户 JWT）—— 回显当前用户 + 组织。
pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<MeResp>> {
    let db = state.db()?;
    let claims = verify_request(&state, &headers)?;

    let user = users::Entity::find_by_id(claims.sub)
        .one(db)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if user.status != users::UserStatus::Enabled {
        return Err(AppError::Forbidden);
    }
    let org = organizations::Entity::find_by_id(user.org_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("organization missing for user".into()))?;
    Ok(Json(MeResp {
        user: user.into(),
        org,
    }))
}

/// 签发用户会话 JWT（HS256，7 天有效）。
fn sign_token(
    secret: &str,
    user_id: i32,
    org_id: i32,
    now: DateTimeWithTimeZone,
) -> AppResult<String> {
    let iat = now.timestamp().max(0) as usize;
    let exp = (now.timestamp() + SESSION_TTL_SECS).max(0) as usize;
    let claims = UserClaims {
        sub: user_id,
        org: org_id,
        iat,
        exp,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("jwt sign: {e}")))
}

/// 校验用户会话 JWT，失败 → 401。
fn verify_token(secret: &str, token: &str) -> AppResult<UserClaims> {
    decode::<UserClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|d| d.claims)
    .map_err(|_| AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_validation() {
        assert!(valid_phone("13800138000"));
        assert!(valid_phone("19912345678"));
        assert!(!valid_phone("12345678901")); // 次位 2
        assert!(!valid_phone("1380013800")); // 10 位
        assert!(!valid_phone("23800138000")); // 首位非 1
        assert!(!valid_phone("1380013800a")); // 非数字
    }

    #[test]
    fn code_is_six_digits_and_phone_bound() {
        let c = generate_code();
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        // 同码不同手机号 → 哈希不同（防跨号复用）
        assert_ne!(code_hash("13800138000", &c), code_hash("13900139000", &c));
    }

    #[test]
    fn jwt_round_trip_and_reject_wrong_secret() {
        let now = chrono::Utc::now().fixed_offset();
        let token = sign_token("s3cr3t", 42, 7, now).unwrap();
        let claims = verify_token("s3cr3t", &token).unwrap();
        assert_eq!(claims.sub, 42);
        assert_eq!(claims.org, 7);
        assert!(verify_token("wrong-secret", &token).is_err());
    }
}
