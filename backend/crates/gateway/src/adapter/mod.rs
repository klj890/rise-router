//! 协议族适配器：把入站 OpenAI 兼容请求转成上游线缆格式，再把上游响应（含每帧 SSE、
//! 错误体）转回 OpenAI 形态。整个鉴权/路由/failover/计费骨架因此与协议无关。
//!
//! 设计要点（docs/architecture.md「多模态网关层」）：
//! - **入站固定 OpenAI 兼容**（对外事实标准）；adapter 只做 OpenAI↔上游双向转换。
//! - **trait 全同步**：所有转换都是纯 CPU（拼 URL / JSON↔JSON / SSE 帧↔SSE 帧）；网络 I/O
//!   留在编排层（relay）。无 async 方法 → 在 `async_stream::stream!` 里分派无对象安全问题。
//! - 流式逻辑下沉为有状态同步转码器 [`SseTranscoder`]（无借用 → `Send + 'static`），可 move
//!   进流生成器。OpenAI 兼容上游走字节透传（[`PassthroughTranscoder`]），可证零回归。
//! - 本 trait **只服务 chat completions**；未来 embedding/image 等用独立小 trait（按
//!   `model.modality` 分派），不膨胀本接口。

use axum::http::{HeaderName, HeaderValue, StatusCode};
use serde_json::Value;

mod anthropic;
mod gemini;
mod openai;
mod sse;

pub use anthropic::AnthropicAdapter;
pub use gemini::GeminiAdapter;
pub use openai::OpenAiCompatAdapter;
pub use sse::{sse_data_payload, sse_done, sse_frame, LineBuffer};

/// `channel.adapter_config`（jsonb）只读视图：协议族内消化厂商 quirk 的配置开关。
pub struct AdapterConfig<'a> {
    raw: Option<&'a Value>,
}

impl<'a> AdapterConfig<'a> {
    pub fn new(raw: Option<&'a Value>) -> Self {
        Self { raw }
    }

    /// 任意厂商开关（字符串值）。
    pub fn get_str(&self, key: &str) -> Option<&'a str> {
        self.raw?.get(key)?.as_str()
    }

    /// 自定义请求 path / 模型名占位符模板（如 Gemini `/v1beta/models/{model}:{action}`）。
    pub fn path_template(&self) -> Option<&'a str> {
        self.get_str("path_template")
    }

    /// Anthropic `anthropic-version` 头，默认稳定版。
    pub fn anthropic_version(&self) -> &'a str {
        self.get_str("anthropic_version").unwrap_or("2023-06-01")
    }

    /// Anthropic `max_tokens` 必填项兜底（OpenAI 选填 → 缺失时用此值）。
    pub fn default_max_tokens(&self) -> i64 {
        self.raw
            .and_then(|v| v.get("default_max_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(4096)
    }
}

/// 协议族适配器：一次 chat 转发的「转换」职责（收发与 failover 在 relay 编排层）。
pub trait ProtocolAdapter: Send + Sync {
    /// 上游请求 URL（base_url + 协议路径 + 占位符替换）。
    fn request_url(
        &self,
        base_url: &str,
        upstream_model: &str,
        is_stream: bool,
        cfg: &AdapterConfig,
    ) -> String;

    /// 鉴权 + 协议固定头（key 取自 `channel.credentials`）。
    fn auth_headers(&self, key: &str, cfg: &AdapterConfig) -> Vec<(HeaderName, HeaderValue)>;

    /// 入站 OpenAI 请求体 → 上游线缆请求体（写入 upstream_model；OpenAI 兼容仅改 model）。
    fn build_request_body(
        &self,
        openai_req: &Value,
        upstream_model: &str,
        cfg: &AdapterConfig,
    ) -> Value;

    /// 上游非流式响应体 → OpenAI 形态。
    /// `None` = 已是 OpenAI 形态、直接用原始字节（OpenAI 兼容走此路，保证零回归）。
    fn convert_response(&self, upstream: &Value, cfg: &AdapterConfig) -> Option<Value>;

    /// 上游非 2xx 错误体 → OpenAI 错误格式 `{error:{message,type,code}}`。
    /// `None` = 原样透传上游错误字节（OpenAI 兼容走此路，零回归）。
    fn convert_error(
        &self,
        status: StatusCode,
        upstream_body: &[u8],
        cfg: &AdapterConfig,
    ) -> Option<Value>;

    /// 流式转码器：上游 SSE 字节 → OpenAI SSE 字节（含末块 usage 与 `[DONE]`）。
    fn stream_transcoder(&self, cfg: &AdapterConfig) -> Box<dyn SseTranscoder + Send>;
}

/// 有状态同步转码器（跨 chunk 自缓冲；move 进 `stream!` 生成器 → 必须 `Send + 'static`）。
pub trait SseTranscoder {
    /// 喂上游字节，吐 OpenAI SSE 字节（可空：等到完整帧再产出）。
    fn push(&mut self, upstream: &[u8]) -> Vec<u8>;
    /// 流结束：补发收尾帧（末块 usage / `data: [DONE]`）。
    fn finish(&mut self) -> Vec<u8>;
}

/// 字节恒等转码器：OpenAI 兼容上游用（上游本就是 OpenAI SSE，原样转发）。
pub struct PassthroughTranscoder;

impl SseTranscoder for PassthroughTranscoder {
    fn push(&mut self, upstream: &[u8]) -> Vec<u8> {
        upstream.to_vec()
    }
    fn finish(&mut self) -> Vec<u8> {
        Vec::new()
    }
}

/// 按 `channel.protocol_adapter` 选适配器。未知协议族返回 None（CRUD 白名单已拦截，此处兜底）。
pub fn adapter_for(protocol: &str) -> Option<Box<dyn ProtocolAdapter>> {
    match protocol {
        "openai_compatible" => Some(Box::new(OpenAiCompatAdapter)),
        "anthropic" => Some(Box::new(AnthropicAdapter)),
        "gemini" => Some(Box::new(GeminiAdapter)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_is_byte_identity() {
        // 零回归锁：OpenAI 路径转码必须字节恒等（push 原样、finish 空），
        // 否则现有 UsageScanner/SettleGuard/计费的输入会漂移。
        let mut t = PassthroughTranscoder;
        let chunk = b"data: {\"id\":\"x\"}\n\n";
        assert_eq!(t.push(chunk), chunk.to_vec());
        assert!(t.finish().is_empty());
    }

    #[test]
    fn adapter_for_known_and_unknown() {
        assert!(adapter_for("openai_compatible").is_some());
        assert!(adapter_for("nope").is_none());
    }

    #[test]
    fn adapter_config_defaults() {
        let cfg = AdapterConfig::new(None);
        assert_eq!(cfg.anthropic_version(), "2023-06-01");
        assert_eq!(cfg.default_max_tokens(), 4096);
        assert!(cfg.path_template().is_none());

        let raw =
            serde_json::json!({"anthropic_version": "2024-01-01", "default_max_tokens": 8192});
        let cfg = AdapterConfig::new(Some(&raw));
        assert_eq!(cfg.anthropic_version(), "2024-01-01");
        assert_eq!(cfg.default_max_tokens(), 8192);
    }
}
