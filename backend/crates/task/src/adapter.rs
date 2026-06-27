//! 任务式厂商适配器（M5a 片B）：按 `channel.protocol_adapter` 分派。
//!
//! 与 chat 的 `ProtocolAdapter`（纯同步转换 + relay 编排 I/O）不同：任务厂商调用是
//! 粗粒度请求/响应（提交 → 拿 vendor_task_id → 轮询），故 trait 用 async 方法自带 I/O，
//! 编排（队列/状态机/落库/计费）仍在 worker。计费量纲由 worker 按任务类型算，适配器不碰钱。

use async_trait::async_trait;
use serde_json::{json, Value};

/// 提交上下文（worker 注入）。
pub struct SubmitCtx<'a> {
    pub http: &'a reqwest::Client,
    pub base_url: &'a str,
    /// 渠道凭据 key（`channel.credentials.key`）
    pub key: &'a str,
    pub upstream_model: &'a str,
    pub task_type: &'a str,
    pub input: &'a Value,
    pub extra: Option<&'a Value>,
}

/// 轮询上下文。
pub struct PollCtx<'a> {
    pub http: &'a reqwest::Client,
    pub base_url: &'a str,
    pub key: &'a str,
    pub vendor_task_id: &'a str,
    /// 已轮询次数（worker 维护，用于 mock 推进 / 超时判定）
    pub poll_count: i32,
}

/// 一次轮询的判定结果。
pub enum TaskPoll {
    Running,
    Succeeded { artifacts: Vec<ProducedArtifact> },
    Failed { message: String },
}

/// 上游产物：可下载 URL（worker 下载后转存）或内联字节（mock / 小产物）。
pub enum ProducedArtifact {
    Url {
        url: String,
        content_type: String,
        meta: Value,
    },
    Bytes {
        bytes: Vec<u8>,
        content_type: String,
        meta: Value,
    },
}

/// 任务式厂商适配器。submit/poll 自带 HTTP I/O；纯请求/响应，无对象安全/借用问题。
#[async_trait]
pub trait TaskAdapter: Send + Sync {
    /// 提交任务 → 返回 vendor_task_id。
    async fn submit(&self, ctx: &SubmitCtx<'_>) -> Result<String, String>;
    /// 轮询任务状态。
    async fn poll(&self, ctx: &PollCtx<'_>) -> Result<TaskPoll, String>;
    /// 取消上游任务（尽力而为）。默认 no-op（不支持取消的厂商）；支持的覆写。
    async fn cancel(&self, _ctx: &PollCtx<'_>) -> Result<(), String> {
        Ok(())
    }
}

/// 按协议族选适配器。未知返回 None（worker 据此把任务置 Failed）。
pub fn adapter_for(protocol: &str) -> Option<Box<dyn TaskAdapter>> {
    match protocol {
        "mock_task" => Some(Box::new(MockTaskAdapter)),
        _ => None,
    }
}

/// Mock 厂商适配器：零凭据、零外呼，用于端到端打通管线。
/// submit 立即返回合成 id；poll 第 2 次起返回成功，产物为内联合成字节。
pub struct MockTaskAdapter;

#[async_trait]
impl TaskAdapter for MockTaskAdapter {
    async fn submit(&self, _ctx: &SubmitCtx<'_>) -> Result<String, String> {
        let suffix: u32 = rand::random();
        Ok(format!("mock-{suffix:08x}"))
    }

    async fn poll(&self, ctx: &PollCtx<'_>) -> Result<TaskPoll, String> {
        // 模拟处理时延：前两次轮询仍在跑，之后产出。
        if ctx.poll_count < 2 {
            return Ok(TaskPoll::Running);
        }
        let body = json!({
            "vendor_task_id": ctx.vendor_task_id,
            "note": "synthetic artifact produced by mock_task adapter",
        });
        Ok(TaskPoll::Succeeded {
            artifacts: vec![ProducedArtifact::Bytes {
                bytes: serde_json::to_vec(&body).unwrap_or_default(),
                content_type: "application/json".into(),
                meta: json!({ "mock": true }),
            }],
        })
    }
}
