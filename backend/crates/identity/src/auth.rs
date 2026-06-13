//! 密钥鉴权：哈希 + 状态/过期/预算校验。纯部分（evaluate_key）单测覆盖。
use rise_entity::api_keys;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// 对原始密钥做 sha256 → 小写 hex。库里只存此哈希，不存明文。
/// 鉴权热路径：单次预分配 64 字节，避免逐字节 format! 的多次小分配。
pub fn hash_key(raw: &str) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(raw.as_bytes());
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// 密钥被拒原因（由 verify_key 映射到 HTTP 状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    Disabled,
    Exhausted,
    Expired,
    BudgetExceeded,
}

/// 鉴权通过后的上下文（无任何密钥/敏感字段，可安全序列化）。
#[derive(Debug, Clone, Serialize)]
pub struct KeyContext {
    pub api_key_id: i32,
    pub org_id: i32,
    pub user_id: Option<i32>,
    /// 组织的商业分组 → 喂给 resolve_price 的 group
    pub group_id: Option<i32>,
    pub allowed_models: Option<serde_json::Value>,
}

/// 纯校验：状态 / 过期 / 预算。无 DB 依赖。
pub fn evaluate_key(key: &api_keys::Model, now: DateTimeWithTimeZone) -> Result<(), KeyError> {
    match key.status {
        api_keys::KeyStatus::Enabled => {}
        api_keys::KeyStatus::Disabled => return Err(KeyError::Disabled),
        api_keys::KeyStatus::Exhausted => return Err(KeyError::Exhausted),
    }
    if let Some(exp) = key.expires_at {
        if exp <= now {
            return Err(KeyError::Expired);
        }
    }
    if let Some(limit) = key.budget_limit {
        if key.budget_used >= limit {
            return Err(KeyError::BudgetExceeded);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;

    fn at(y: i32, mo: u32, d: u32) -> DateTimeWithTimeZone {
        Utc.with_ymd_and_hms(y, mo, d, 0, 0, 0)
            .unwrap()
            .fixed_offset()
    }

    fn key(status: api_keys::KeyStatus) -> api_keys::Model {
        api_keys::Model {
            id: 1,
            org_id: 1,
            user_id: None,
            app_id: None,
            key_hash: "h".into(),
            name: "k".into(),
            allowed_models: None,
            budget_limit: None,
            budget_used: Decimal::ZERO,
            expires_at: None,
            status,
        }
    }

    #[test]
    fn enabled_no_limits_passes() {
        assert!(evaluate_key(&key(api_keys::KeyStatus::Enabled), at(2026, 6, 1)).is_ok());
    }

    #[test]
    fn disabled_rejected() {
        assert_eq!(
            evaluate_key(&key(api_keys::KeyStatus::Disabled), at(2026, 6, 1)),
            Err(KeyError::Disabled)
        );
    }

    #[test]
    fn exhausted_rejected() {
        assert_eq!(
            evaluate_key(&key(api_keys::KeyStatus::Exhausted), at(2026, 6, 1)),
            Err(KeyError::Exhausted)
        );
    }

    #[test]
    fn expired_rejected_inclusive() {
        let mut k = key(api_keys::KeyStatus::Enabled);
        k.expires_at = Some(at(2026, 3, 1));
        assert_eq!(evaluate_key(&k, at(2026, 6, 1)), Err(KeyError::Expired));
        // 未到期则通过
        k.expires_at = Some(at(2026, 9, 1));
        assert!(evaluate_key(&k, at(2026, 6, 1)).is_ok());
    }

    #[test]
    fn budget_exceeded_rejected() {
        let mut k = key(api_keys::KeyStatus::Enabled);
        k.budget_limit = Some(Decimal::new(100, 0));
        k.budget_used = Decimal::new(100, 0); // 用满即拒（>=）
        assert_eq!(
            evaluate_key(&k, at(2026, 6, 1)),
            Err(KeyError::BudgetExceeded)
        );
        k.budget_used = Decimal::new(9999, 2); // 99.99 < 100 → 通过
        assert!(evaluate_key(&k, at(2026, 6, 1)).is_ok());
    }

    #[test]
    fn hash_key_is_sha256_hex() {
        // 已知向量：sha256("foo")
        assert_eq!(
            hash_key("foo"),
            "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae"
        );
    }
}
