//! 管理鉴权守卫 `require`：RBAC 落地后管理端点的统一入口。
//!
//! 两条放行通道：
//! 1. **superadmin 逃生通道**——携带有效 `X-Admin-Token`（匹配 `RR_ADMIN_TOKEN`）即放行，绕过角色检查
//!    （运维/引导用；RBAC 完整运营后可收紧或关闭）。
//! 2. **用户 RBAC**——Bearer 用户 JWT → 解析其角色权限码 → 含所需权限点 `perm` 则放行，否则 403。
//!
//! 守卫放在 identity 域（这里有 JWT 校验），调用 rbac 域的权限解析；依赖方向 identity→rbac，无环。

use axum::http::HeaderMap;
use rise_core::{AppError, AppResult, AppState};

/// 鉴权通过的主体。多数 handler 不关心具体主体，仅用 `?` 做授权门禁；
/// 需要"操作者"上下文（如审计、按 org 归属）的 handler 可取用。
#[derive(Debug, Clone, Copy)]
pub enum Subject {
    /// 持管理令牌的超级管理员（逃生通道）
    SuperAdmin,
    /// 通过角色权限校验的登录用户
    User { user_id: i32, org_id: i32 },
}

/// 要求调用方具备权限点 `perm`。superadmin 令牌直接放行；否则校验用户 JWT + RBAC 权限。
///
/// 失败映射：无任何凭据 → 401；JWT 有效但缺权限 → 403；未配 RR_JWT_SECRET 且无管理令牌 → 503。
pub async fn require(state: &AppState, headers: &HeaderMap, perm: &str) -> AppResult<Subject> {
    // 通道 1：管理令牌（超管逃生通道）。
    if rise_core::admin_token_ok(state, headers) {
        return Ok(Subject::SuperAdmin);
    }
    // 通道 2：用户 JWT + RBAC 权限点。
    let claims = crate::session::verify_request(state, headers)?;
    let db = state.db()?;
    let perms = rise_rbac::user_permissions(db, claims.sub).await?;
    if rise_rbac::enforce(&perms, perm) {
        Ok(Subject::User {
            user_id: claims.sub,
            org_id: claims.org,
        })
    } else {
        Err(AppError::Forbidden)
    }
}
