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

/// 数据域访问决议：在「能访问」之上区分「能看多大范围」+「操作者是谁」。
///
/// CRM 等带归属边界的域用它做端点层行级隔离（M3；完整 RLS 引擎留 M4）：
/// 普通销售（仅 base_perm）受归属约束，[`owned_by`](Access::owned_by) 返回本人 id 强制过滤；
/// 具 all_perm（管理员/财务）或超管令牌则全量，`owned_by` 返回 `None`。
#[derive(Debug, Clone, Copy)]
pub struct Access {
    /// 是否无归属边界（超管令牌 / 具 all_perm 的用户）
    all: bool,
    /// 操作者 user_id（超管令牌无用户上下文时 None）
    user_id: Option<i32>,
    /// 操作者 org_id（同上）
    org_id: Option<i32>,
}

impl Access {
    /// 是否全量可见（无归属边界）。
    pub fn is_all(&self) -> bool {
        self.all
    }

    /// 受限时返回必须过滤的销售 user_id（`owner_sales_id = ?`）；全量访问返回 `None`（不过滤）。
    pub fn owned_by(&self) -> Option<i32> {
        if self.all {
            None
        } else {
            self.user_id
        }
    }

    /// 操作者 user_id（用于 author_id / created_by 等审计字段；超管令牌为 `None`）。
    pub fn actor_id(&self) -> Option<i32> {
        self.user_id
    }

    /// 操作者 org_id（超管令牌为 `None`）。
    pub fn actor_org(&self) -> Option<i32> {
        self.org_id
    }
}

/// 要求至少具备 `base_perm`；若还具备 `all_perm` 则全量访问，否则限本人名下。
///
/// 超管令牌 → 全量（无用户上下文）。失败映射同 [`require`]（401/403/503）。
pub async fn require_scoped(
    state: &AppState,
    headers: &HeaderMap,
    base_perm: &str,
    all_perm: &str,
) -> AppResult<Access> {
    // 通道 1：管理令牌（超管逃生通道）→ 全量，无用户上下文。
    if rise_core::admin_token_ok(state, headers) {
        return Ok(Access {
            all: true,
            user_id: None,
            org_id: None,
        });
    }
    // 通道 2：用户 JWT + RBAC 权限点。
    let claims = crate::session::verify_request(state, headers)?;
    let db = state.db()?;
    let perms = rise_rbac::user_permissions(db, claims.sub).await?;
    // 必须具备 base_perm 才放行（与本函数契约一致）；all_perm 仅决定数据域范围（全量/本人名下）。
    // 切勿写成 `all || base`：否则具 all_perm 但无 base_perm 者会越权——例如 finance 持
    // crm.read.all 却无 crm.write，会绕过写端点的 base=crm.write 校验写入跟进记录（权限提升）。
    if !rise_rbac::enforce(&perms, base_perm) {
        return Err(AppError::Forbidden);
    }
    let all = rise_rbac::enforce(&perms, all_perm);
    Ok(Access {
        all,
        user_id: Some(claims.sub),
        org_id: Some(claims.org),
    })
}
