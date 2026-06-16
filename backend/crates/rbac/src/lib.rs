//! RBAC 域：角色/权限点目录 + enforce + 用户权限解析 + 内置 seed + 授角色。
//!
//! 权限点与内置角色以**代码常量为源**（[`PERMISSIONS`]/[`ROLES`]/[`permissions_for_role`]），
//! [`seed_builtins`] 幂等落库为派生缓存（狗粮原则：内部模块也按 App 声明权限点）。
//! 鉴权热路径：[`user_permissions`] 取用户全部权限码，纯函数 [`enforce`] 判定。
//! 管理端点的 `require` 守卫在 identity 域（那里有 JWT），调用本域的解析；避免 identity↔rbac 循环依赖。

use std::collections::HashSet;

use rise_entity::{permissions, role_permissions, roles, user_roles};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set,
};

mod routes;
pub use routes::routes;

/// 权限点目录：(code, module, description)。内部模块声明，seed 落库。
pub const PERMISSIONS: &[(&str, &str, &str)] = &[
    ("gateway.manage", "gateway", "渠道 / 模型 / 路由线 管理"),
    ("pricing.manage", "pricing", "分组 / 价格 / 折扣 管理"),
    ("identity.manage", "identity", "组织 / 密钥 管理"),
    ("billing.manage", "billing", "充值 / 订单 / 对账 管理"),
    ("rbac.manage", "rbac", "角色授予 / 权限查看 管理"),
    ("crm.read", "crm", "客户档案 / 跟进 / 归属 查看（本人名下）"),
    ("crm.read.all", "crm", "跨销售查看全部客户（无归属边界）"),
    ("crm.write", "crm", "跟进记录 / 代客操作 写入（本人名下）"),
    ("crm.assign", "crm", "客户归属销售 变更（管理员级）"),
];

/// 内置角色：(slug, name)。
pub const ROLES: &[(&str, &str)] = &[
    ("admin", "管理员"),
    ("finance", "财务"),
    ("ops", "运维"),
    ("sales", "销售"),
    ("customer", "客户"),
];

/// 角色 → 权限点（admin 全量；其余按业务域；customer 无管理权限）。
pub fn permissions_for_role(role_slug: &str) -> Vec<&'static str> {
    match role_slug {
        "admin" => PERMISSIONS.iter().map(|(c, _, _)| *c).collect(),
        // 财务看全量客户业绩（read + read.all），但不代客操作、不改归属
        "finance" => vec![
            "billing.manage",
            "pricing.manage",
            "crm.read",
            "crm.read.all",
        ],
        "ops" => vec!["gateway.manage"],
        // 销售：管客户（建组织/密钥）+ 看/写自己名下客户（crm.read/write，无 read.all → 受归属边界约束）
        "sales" => vec!["identity.manage", "crm.read", "crm.write"],
        _ => vec![],
    }
}

/// 纯判定：主体权限集是否含所需权限码。
pub fn enforce(perms: &HashSet<String>, required: &str) -> bool {
    perms.contains(required)
}

/// 解析用户的全部权限码（user_roles → role_permissions → permissions.code）。
pub async fn user_permissions(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<HashSet<String>, DbErr> {
    let role_ids: Vec<i32> = user_roles::Entity::find()
        .filter(user_roles::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|r| r.role_id)
        .collect();
    if role_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let perm_ids: Vec<i32> = role_permissions::Entity::find()
        .filter(role_permissions::Column::RoleId.is_in(role_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|rp| rp.permission_id)
        .collect();
    if perm_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let codes: HashSet<String> = permissions::Entity::find()
        .filter(permissions::Column::Id.is_in(perm_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|p| p.code)
        .collect();
    Ok(codes)
}

/// 幂等落地内置角色 / 权限点 / 角色↔权限点。启动时调用（数据驱动，重放安全）。
pub async fn seed_builtins(db: &DatabaseConnection) -> Result<(), DbErr> {
    for (code, module, desc) in PERMISSIONS {
        if permissions::find_by_code(db, code).await?.is_none() {
            permissions::ActiveModel {
                code: Set((*code).to_owned()),
                module: Set(Some((*module).to_owned())),
                description: Set(Some((*desc).to_owned())),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }
    }
    for (slug, name) in ROLES {
        if roles::find_by_slug(db, slug).await?.is_none() {
            roles::ActiveModel {
                slug: Set((*slug).to_owned()),
                name: Set((*name).to_owned()),
                is_builtin: Set(true),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }
    }
    for (slug, _) in ROLES {
        let Some(role) = roles::find_by_slug(db, slug).await? else {
            continue;
        };
        for code in permissions_for_role(slug) {
            let Some(perm) = permissions::find_by_code(db, code).await? else {
                continue;
            };
            if role_permissions::Entity::find_by_id((role.id, perm.id))
                .one(db)
                .await?
                .is_none()
            {
                role_permissions::ActiveModel {
                    role_id: Set(role.id),
                    permission_id: Set(perm.id),
                }
                .insert(db)
                .await?;
            }
        }
    }
    tracing::info!("rbac builtins seeded");
    Ok(())
}

/// 幂等给用户授角色（按 role slug）。角色不存在则跳过（seed 未跑）。
pub async fn grant_role(
    db: &DatabaseConnection,
    user_id: i32,
    role_slug: &str,
) -> Result<(), DbErr> {
    let Some(role) = roles::find_by_slug(db, role_slug).await? else {
        return Ok(());
    };
    let exists = user_roles::Entity::find()
        .filter(user_roles::Column::UserId.eq(user_id))
        .filter(user_roles::Column::RoleId.eq(role.id))
        .one(db)
        .await?
        .is_some();
    if !exists {
        user_roles::ActiveModel {
            user_id: Set(user_id),
            role_id: Set(role.id),
            scope: Set(None),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

/// 幂等撤销用户的某角色（按 slug）。角色不存在或未授均视作成功（no-op）。
pub async fn revoke_role(
    db: &DatabaseConnection,
    user_id: i32,
    role_slug: &str,
) -> Result<(), DbErr> {
    let Some(role) = roles::find_by_slug(db, role_slug).await? else {
        return Ok(());
    };
    user_roles::Entity::delete_many()
        .filter(user_roles::Column::UserId.eq(user_id))
        .filter(user_roles::Column::RoleId.eq(role.id))
        .exec(db)
        .await?;
    Ok(())
}

/// 列出全部角色（管理台展示）。
pub async fn list_roles(db: &DatabaseConnection) -> Result<Vec<roles::Model>, DbErr> {
    roles::Entity::find()
        .order_by_asc(roles::Column::Id)
        .all(db)
        .await
}

/// 列出全部权限点目录（管理台展示）。
pub async fn list_permissions(db: &DatabaseConnection) -> Result<Vec<permissions::Model>, DbErr> {
    permissions::Entity::find()
        .order_by_asc(permissions::Column::Id)
        .all(db)
        .await
}

/// 列出某用户已授的角色。
pub async fn list_user_roles(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<Vec<roles::Model>, DbErr> {
    let role_ids: Vec<i32> = user_roles::Entity::find()
        .filter(user_roles::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|r| r.role_id)
        .collect();
    if role_ids.is_empty() {
        return Ok(vec![]);
    }
    roles::Entity::find()
        .filter(roles::Column::Id.is_in(role_ids))
        .order_by_asc(roles::Column::Id)
        .all(db)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforce_checks_membership() {
        let mut perms = HashSet::new();
        perms.insert("pricing.manage".to_owned());
        assert!(enforce(&perms, "pricing.manage"));
        assert!(!enforce(&perms, "billing.manage"));
    }

    #[test]
    fn admin_gets_all_perms_others_subset() {
        let all: Vec<&str> = PERMISSIONS.iter().map(|(c, _, _)| *c).collect();
        assert_eq!(permissions_for_role("admin").len(), all.len());
        assert_eq!(permissions_for_role("ops"), vec!["gateway.manage"]);
        assert!(permissions_for_role("customer").is_empty());
        // 每个非 admin 角色的权限都是合法权限码
        for slug in ["finance", "ops", "sales"] {
            for c in permissions_for_role(slug) {
                assert!(all.contains(&c), "{c} not in catalog");
            }
        }
    }
}
