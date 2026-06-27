//! 多模态异步任务（M5a）状态机实体。无密钥字段，可直接序列化为 API 响应。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub org_id: i32,
    pub api_key_id: i32,
    pub user_id: Option<i32>,
    pub app_id: Option<i32>,
    /// 任务类型：video.generation / image.generation / audio.speech …（列名 `type`，API 输出 `type`）
    #[sea_orm(column_name = "type")]
    #[serde(rename = "type")]
    pub task_type: String,
    pub model_id: i32,
    pub model_slug: String,
    pub channel_id: Option<i32>,
    pub group_slug: Option<String>,
    pub status: TaskStatus,
    pub input: Json,
    pub extra: Option<Json>,
    pub vendor_task_id: Option<String>,
    pub usage: Option<Json>,
    pub base_amount: Option<Decimal>,
    pub charged_amount: Option<Decimal>,
    pub error: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_state: Option<String>,
    pub request_id: Option<String>,
    pub poll_count: i32,
    pub created_at: DateTimeWithTimeZone,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub updated_at: DateTimeWithTimeZone,
}

/// 任务状态机（强类型，映射 smallint）。序列化为变体名（queued/running/…）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[sea_orm(num_value = 1)]
    Queued,
    #[sea_orm(num_value = 2)]
    Running,
    #[sea_orm(num_value = 3)]
    Succeeded,
    #[sea_orm(num_value = 4)]
    Failed,
    #[sea_orm(num_value = 5)]
    Cancelled,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
