//! 临时管理守卫（RBAC 落地前的过渡件）。
//!
//! 管理类写端点（手动充值 / 渠道·模型·价格·分组·密钥 CRUD 等）在 RBAC 角色系统落地前，
//! 统一用 `X-Admin-Token` 头匹配配置项 `RR_ADMIN_TOKEN`（常量时间比较）守卫；
//! **未配置 token 则一律 403**（安全默认：不显式开启管理口令，就不开放任何管理端点）。
//! RBAC（roles/permissions）落地后，此守卫整体替换为基于角色/权限点的鉴权。

use crate::{AppError, AppResult, AppState};
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

/// 校验请求携带的管理令牌：`X-Admin-Token` 头匹配 `RR_ADMIN_TOKEN`（常量时间比较）；
/// 未配置或不匹配则 [`AppError::Forbidden`]。所有域的管理 CRUD 端点共用此守卫。
pub fn admin_guard(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    let admin = state
        .config
        .admin_token
        .as_deref()
        .ok_or(AppError::Forbidden)?;
    let provided = headers
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Forbidden)?;
    if token_eq(provided, admin) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// 令牌比较：先各自 SHA-256 再常量时间比较定长 32 字节摘要。
/// 哈希后长度恒为 32，消除「长度不等早返回」泄露 token 长度的计时侧信道。
fn token_eq(provided: &str, expected: &str) -> bool {
    let a = Sha256::digest(provided.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::token_eq;

    #[test]
    fn token_eq_matches_identical_and_rejects_others() {
        assert!(token_eq("s3cr3t", "s3cr3t"));
        assert!(!token_eq("s3cr3t", "s3cr3T"));
        // 长度不同也安全比较（哈希后定长），不应 panic、不应判等。
        assert!(!token_eq("short", "a-much-longer-token"));
        assert!(!token_eq("", "nonempty"));
    }
}
