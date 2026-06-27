//! OpenAI 兼容适配器：入站本就是 OpenAI 形态，故对 OpenAI 兼容上游**纯透传**——
//! 仅改写 `model` 为上游真实名。`convert_response`/`convert_error` 返回 `None`（用原始字节）、
//! `stream_transcoder` 用 [`PassthroughTranscoder`]，三者合力保证字节级零回归。

use axum::http::{header::AUTHORIZATION, HeaderName, HeaderValue, StatusCode};
use serde_json::Value;

use super::{AdapterConfig, PassthroughTranscoder, ProtocolAdapter, SseTranscoder};

pub struct OpenAiCompatAdapter;

impl ProtocolAdapter for OpenAiCompatAdapter {
    fn request_url(
        &self,
        base_url: &str,
        _upstream_model: &str,
        _is_stream: bool,
        _cfg: &AdapterConfig,
    ) -> String {
        format!("{}/chat/completions", base_url.trim_end_matches('/'))
    }

    fn auth_headers(&self, key: &str, _cfg: &AdapterConfig) -> Vec<(HeaderName, HeaderValue)> {
        // 非法 key（含控制字符等）无法构成头值时跳过，让上游返 401（与原 bearer_auth 行为一致）。
        match HeaderValue::from_str(&format!("Bearer {key}")) {
            Ok(hv) => vec![(AUTHORIZATION, hv)],
            Err(_) => Vec::new(),
        }
    }

    fn build_request_body(
        &self,
        openai_req: &Value,
        upstream_model: &str,
        _cfg: &AdapterConfig,
    ) -> Value {
        let mut body = openai_req.clone();
        body["model"] = Value::String(upstream_model.to_string());
        body
    }

    fn convert_response(&self, _upstream: &Value, _cfg: &AdapterConfig) -> Option<Value> {
        None // 已是 OpenAI 形态 → relay 用原始字节，零回归
    }

    fn convert_error(
        &self,
        _status: StatusCode,
        _upstream_body: &[u8],
        _cfg: &AdapterConfig,
    ) -> Option<Value> {
        None // 上游错误已是 OpenAI 形态 → 原样透传，零回归
    }

    fn stream_transcoder(&self, _cfg: &AdapterConfig) -> Box<dyn SseTranscoder + Send> {
        Box::new(PassthroughTranscoder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> AdapterConfig<'static> {
        AdapterConfig::new(None)
    }

    #[test]
    fn url_trims_trailing_slash() {
        let a = OpenAiCompatAdapter;
        assert_eq!(
            a.request_url("https://api.x.com/v1/", "m", false, &cfg()),
            "https://api.x.com/v1/chat/completions"
        );
    }

    #[test]
    fn auth_is_bearer() {
        let a = OpenAiCompatAdapter;
        let h = a.auth_headers("sk-abc", &cfg());
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0, AUTHORIZATION);
        assert_eq!(h[0].1, HeaderValue::from_static("Bearer sk-abc"));
    }

    #[test]
    fn build_body_only_rewrites_model() {
        let a = OpenAiCompatAdapter;
        let req = json!({"model": "alias", "messages": [{"role": "user", "content": "hi"}], "stream": true});
        let out = a.build_request_body(&req, "gpt-4o-real", &cfg());
        assert_eq!(out["model"], json!("gpt-4o-real"));
        // 其余字段原样保留
        assert_eq!(out["messages"], req["messages"]);
        assert_eq!(out["stream"], json!(true));
    }

    #[test]
    fn passthrough_returns_none_for_zero_regression() {
        let a = OpenAiCompatAdapter;
        assert!(a.convert_response(&json!({"usage": {}}), &cfg()).is_none());
        assert!(a
            .convert_error(StatusCode::BAD_REQUEST, b"{\"error\":{}}", &cfg())
            .is_none());
    }
}
