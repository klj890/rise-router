//! 组织（账户与计费主体）。商业分组 group_id 挂这里（定价五要素之一）。
//! 个人自主注册 = type=个人 的 org-of-one，统一计费模型。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub org_type: OrgType,
    /// 商业分组（定价档位）；空=按默认价计费（feeds resolve_price 的 group）
    pub group_id: Option<i32>,
    pub status: OrgStatus,
    pub realname_status: RealnameStatus,
    /// 归属销售（CRM，可空；FK 待 users 表落地后补）
    pub owner_sales_id: Option<i32>,
}

/// 组织类型（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum OrgType {
    #[sea_orm(num_value = 1)]
    Individual,
    #[sea_orm(num_value = 2)]
    Enterprise,
}

/// 组织状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum OrgStatus {
    #[sea_orm(num_value = 1)]
    Active,
    #[sea_orm(num_value = 2)]
    Suspended,
}

/// 实名认证状态（强类型，映射 smallint）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
pub enum RealnameStatus {
    #[sea_orm(num_value = 0)]
    Unverified,
    #[sea_orm(num_value = 1)]
    IndividualVerified,
    #[sea_orm(num_value = 2)]
    EnterpriseVerified,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
