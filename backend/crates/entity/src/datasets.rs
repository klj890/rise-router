//! 报表数据集（M4 监控报表域）：策展语义层，**不开放原始库**。
//!
//! 每个数据集声明一个白名单 `source`（代码注册的策展视图/表）+ 可用 `metrics`/`dimensions`
//! （管理员从 source 允许集中策展的子集）+ `rls_rule`（按角色的行级过滤分支）。
//! 报表（`report_definitions`）只能基于数据集搭建，碰不到原始表；查询引擎执行时按当前用户
//! 角色强制注入 `rls_rule` 对应分支，用户无法绕过。详见 docs/data-model.md §⑨。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "datasets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 数据集标识（唯一，如 usage/revenue/sales_perf/channel_health）
    pub slug: String,
    pub name: String,
    /// 白名单 source 注册键（代码侧 `report::source` 决定物理视图与可用列）
    pub source: String,
    /// 可用指标：[{key,label}]（key 须在 source 的指标白名单内）
    pub metrics: Json,
    /// 可用维度：[{key,label}]（key 须在 source 的维度白名单内）
    pub dimensions: Json,
    /// 行级安全规则：{role: {column,param} | null}。null = 该角色全量；缺键 = 该角色禁止访问。
    pub rls_rule: Json,
    /// 访问所需权限点（如 report.read / report.dataset.finance）
    pub required_permission: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 按 slug 查数据集。
pub async fn find_by_slug<C: ConnectionTrait>(db: &C, slug: &str) -> Result<Option<Model>, DbErr> {
    Entity::find().filter(Column::Slug.eq(slug)).one(db).await
}
