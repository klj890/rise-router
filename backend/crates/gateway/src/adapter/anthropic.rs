//! Anthropic Messages 协议适配器：OpenAI chat ↔ Anthropic `/v1/messages` 双向转换
//! （含 tools/function calling、多模态 image、错误归一、流式 SSE 状态机）。
//!
//! 上游响应统一转回 OpenAI 形态，使现有 UsageScanner/计费/SettleGuard 无感知。

use std::collections::HashMap;

use axum::http::{HeaderName, HeaderValue, StatusCode};
use serde_json::{json, Map, Value};

use super::{
    sse_data_payload, sse_done, sse_frame, AdapterConfig, LineBuffer, ProtocolAdapter,
    SseTranscoder,
};

pub struct AnthropicAdapter;

impl ProtocolAdapter for AnthropicAdapter {
    fn request_url(
        &self,
        base_url: &str,
        _upstream_model: &str,
        _is_stream: bool,
        _cfg: &AdapterConfig,
    ) -> String {
        format!("{}/v1/messages", base_url.trim_end_matches('/'))
    }

    fn auth_headers(&self, key: &str, cfg: &AdapterConfig) -> Vec<(HeaderName, HeaderValue)> {
        let mut out = Vec::new();
        // Anthropic 用 x-api-key（非 Bearer）+ anthropic-version
        if let Ok(v) = HeaderValue::from_str(key) {
            out.push((HeaderName::from_static("x-api-key"), v));
        }
        if let Ok(v) = HeaderValue::from_str(cfg.anthropic_version()) {
            out.push((HeaderName::from_static("anthropic-version"), v));
        }
        out
    }

    fn build_request_body(
        &self,
        openai_req: &Value,
        upstream_model: &str,
        cfg: &AdapterConfig,
    ) -> Value {
        let mut out = Map::new();
        out.insert("model".into(), json!(upstream_model));

        // max_tokens 是 Anthropic 必填项：OpenAI 选填 → 缺失时用配置兜底
        let max_tokens = openai_req
            .get("max_tokens")
            .and_then(Value::as_i64)
            .or_else(|| {
                openai_req
                    .get("max_completion_tokens")
                    .and_then(Value::as_i64)
            })
            .unwrap_or_else(|| cfg.default_max_tokens());
        out.insert("max_tokens".into(), json!(max_tokens));

        // 透传同名标量；剥离 OpenAI 专属字段（stream_options.include_usage 等不传 Anthropic）
        for k in ["temperature", "top_p", "top_k", "stream"] {
            if let Some(v) = openai_req.get(k) {
                out.insert(k.into(), v.clone());
            }
        }
        if let Some(stop) = openai_req.get("stop") {
            let seqs = match stop {
                Value::String(s) => vec![Value::String(s.clone())],
                Value::Array(a) => a.clone(),
                _ => vec![],
            };
            if !seqs.is_empty() {
                out.insert("stop_sequences".into(), Value::Array(seqs));
            }
        }

        let (messages, system) = convert_messages(openai_req.get("messages"));
        if let Some(sys) = system {
            out.insert("system".into(), json!(sys));
        }
        out.insert("messages".into(), Value::Array(messages));

        if let Some(tools) = openai_req.get("tools").and_then(Value::as_array) {
            let conv = convert_tools(tools);
            if !conv.is_empty() {
                out.insert("tools".into(), Value::Array(conv));
            }
        }
        if let Some(mapped) = openai_req.get("tool_choice").and_then(convert_tool_choice) {
            out.insert("tool_choice".into(), mapped);
        }

        Value::Object(out)
    }

    fn convert_response(&self, upstream: &Value, _cfg: &AdapterConfig) -> Option<Value> {
        let id = upstream.get("id").and_then(Value::as_str).unwrap_or("");
        let model = upstream.get("model").and_then(Value::as_str).unwrap_or("");

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        if let Some(blocks) = upstream.get("content").and_then(Value::as_array) {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(Value::as_str) {
                            text.push_str(t);
                        }
                    }
                    Some("tool_use") => tool_calls.push(tool_use_to_openai(b)),
                    _ => {}
                }
            }
        }

        let finish_reason = map_stop_reason(upstream.get("stop_reason").and_then(Value::as_str));
        let (input_t, output_t) = usage_tokens(upstream.get("usage"));

        let mut message = Map::new();
        message.insert("role".into(), json!("assistant"));
        // 纯工具调用时 content 为 null（OpenAI 约定）
        message.insert(
            "content".into(),
            if text.is_empty() && !tool_calls.is_empty() {
                Value::Null
            } else {
                json!(text)
            },
        );
        if !tool_calls.is_empty() {
            message.insert("tool_calls".into(), Value::Array(tool_calls));
        }

        Some(json!({
            "id": id,
            "object": "chat.completion",
            "created": 0,
            "model": model,
            "choices": [{"index": 0, "message": Value::Object(message), "finish_reason": finish_reason}],
            "usage": {
                "prompt_tokens": input_t,
                "completion_tokens": output_t,
                "total_tokens": input_t + output_t,
            },
        }))
    }

    fn convert_error(
        &self,
        status: StatusCode,
        upstream_body: &[u8],
        _cfg: &AdapterConfig,
    ) -> Option<Value> {
        let (message, etype) = match serde_json::from_slice::<Value>(upstream_body) {
            Ok(v) => {
                let err = v.get("error");
                let msg = err
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| String::from_utf8_lossy(upstream_body).into_owned());
                let t = err
                    .and_then(|e| e.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("upstream_error")
                    .to_string();
                (msg, t)
            }
            Err(_) => (
                String::from_utf8_lossy(upstream_body).into_owned(),
                "upstream_error".to_string(),
            ),
        };
        Some(json!({"error": {"message": message, "type": etype, "code": status.as_u16()}}))
    }

    fn stream_transcoder(&self, _cfg: &AdapterConfig) -> Box<dyn SseTranscoder + Send> {
        Box::new(AnthropicTranscoder::default())
    }
}

// ─── 请求转换 helper ────────────────────────────────────────────────────────

/// OpenAI messages → (Anthropic messages, 顶层 system)。
fn convert_messages(messages: Option<&Value>) -> (Vec<Value>, Option<String>) {
    let mut out: Vec<Value> = Vec::new();
    let mut system_parts: Vec<String> = Vec::new();
    let Some(arr) = messages.and_then(Value::as_array) else {
        return (out, None);
    };
    for msg in arr {
        match msg.get("role").and_then(Value::as_str).unwrap_or("") {
            // Anthropic messages 无 system role → 抽到顶层（developer 同 system 处理）
            "system" | "developer" => {
                if let Some(s) = content_to_text(msg.get("content")) {
                    system_parts.push(s);
                }
            }
            "user" => {
                out.push(json!({"role": "user", "content": convert_content(msg.get("content"))}))
            }
            "assistant" => {
                let mut blocks = convert_content(msg.get("content"));
                if let Some(tcs) = msg.get("tool_calls").and_then(Value::as_array) {
                    for tc in tcs {
                        blocks.push(openai_tool_call_to_tool_use(tc));
                    }
                }
                out.push(json!({"role": "assistant", "content": blocks}));
            }
            "tool" => {
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": msg.get("tool_call_id").and_then(Value::as_str).unwrap_or(""),
                    "content": content_to_text(msg.get("content")).unwrap_or_default(),
                });
                // 连续 tool 结果合并进同一 user message（Anthropic 要求 role 交替）
                if append_to_tool_result_message(&mut out, &block) {
                    continue;
                }
                out.push(json!({"role": "user", "content": [block]}));
            }
            _ => {}
        }
    }
    let system = (!system_parts.is_empty()).then(|| system_parts.join("\n\n"));
    (out, system)
}

/// 若 `out` 末条是「由 tool_result 构成的 user message」，把 block 追加进去并返回 true。
fn append_to_tool_result_message(out: &mut [Value], block: &Value) -> bool {
    let Some(last) = out.last_mut() else {
        return false;
    };
    if last.get("role").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let is_tool_agg = last
        .get("content")
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .and_then(|b| b.get("type"))
        .and_then(Value::as_str)
        == Some("tool_result");
    if !is_tool_agg {
        return false;
    }
    if let Some(arr) = last.get_mut("content").and_then(Value::as_array_mut) {
        arr.push(block.clone());
        return true;
    }
    false
}

/// content（string | array）→ Anthropic content blocks（text + image）。
fn convert_content(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(s)) if !s.is_empty() => vec![json!({"type": "text", "text": s})],
        Some(Value::Array(arr)) => {
            let mut blocks = Vec::new();
            for part in arr {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            blocks.push(json!({"type": "text", "text": t}));
                        }
                    }
                    Some("image_url") => {
                        if let Some(url) = part
                            .get("image_url")
                            .and_then(|u| u.get("url"))
                            .and_then(Value::as_str)
                        {
                            blocks.push(image_block(url));
                        }
                    }
                    _ => {}
                }
            }
            blocks
        }
        _ => vec![],
    }
}

/// OpenAI image_url → Anthropic image block（data URI 走 base64 source，否则 url source）。
fn image_block(url: &str) -> Value {
    if let Some(rest) = url.strip_prefix("data:") {
        if let Some((meta, data)) = rest.split_once(',') {
            let media_type = meta.split(';').next().unwrap_or("image/png");
            return json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": data},
            });
        }
    }
    json!({"type": "image", "source": {"type": "url", "url": url}})
}

/// content（string | array）→ 纯文本（拼接 text 片段）。
fn content_to_text(content: Option<&Value>) -> Option<String> {
    match content {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(arr)) => {
            let mut s = String::new();
            for part in arr {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = part.get("text").and_then(Value::as_str) {
                        s.push_str(t);
                    }
                }
            }
            (!s.is_empty()).then_some(s)
        }
        _ => None,
    }
}

fn convert_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            let name = f.get("name").and_then(Value::as_str)?;
            let mut obj = Map::new();
            obj.insert("name".into(), json!(name));
            if let Some(d) = f.get("description") {
                obj.insert("description".into(), d.clone());
            }
            obj.insert(
                "input_schema".into(),
                f.get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"})),
            );
            Some(Value::Object(obj))
        })
        .collect()
}

fn convert_tool_choice(tc: &Value) -> Option<Value> {
    match tc {
        Value::String(s) => match s.as_str() {
            "auto" => Some(json!({"type": "auto"})),
            "required" => Some(json!({"type": "any"})),
            _ => None, // none → Anthropic 无对应，省略
        },
        Value::Object(o) => o
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            .map(|name| json!({"type": "tool", "name": name})),
        _ => None,
    }
}

/// OpenAI assistant tool_call → Anthropic tool_use block。
fn openai_tool_call_to_tool_use(tc: &Value) -> Value {
    let f = tc.get("function");
    let args = f
        .and_then(|f| f.get("arguments"))
        .and_then(Value::as_str)
        .unwrap_or("{}");
    json!({
        "type": "tool_use",
        "id": tc.get("id").and_then(Value::as_str).unwrap_or(""),
        "name": f.and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or(""),
        "input": serde_json::from_str::<Value>(args).unwrap_or_else(|_| json!({})),
    })
}

/// Anthropic tool_use block → OpenAI tool_call。
fn tool_use_to_openai(b: &Value) -> Value {
    let input = b.get("input").cloned().unwrap_or_else(|| json!({}));
    json!({
        "id": b.get("id").and_then(Value::as_str).unwrap_or(""),
        "type": "function",
        "function": {
            "name": b.get("name").and_then(Value::as_str).unwrap_or(""),
            "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()),
        },
    })
}

fn usage_tokens(u: Option<&Value>) -> (i64, i64) {
    let i = u
        .and_then(|u| u.get("input_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let o = u
        .and_then(|u| u.get("output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    (i, o)
}

/// Anthropic stop_reason → OpenAI finish_reason。
fn map_stop_reason(sr: Option<&str>) -> &'static str {
    match sr {
        Some("max_tokens") => "length",
        Some("tool_use") => "tool_calls",
        _ => "stop", // end_turn / stop_sequence / 缺失
    }
}

// ─── 流式 SSE 状态机转码器 ───────────────────────────────────────────────────

/// Anthropic SSE（命名事件）→ OpenAI chat.completion.chunk 流。
/// usage 跨 `message_start`(input) + `message_delta`(output) 两事件累积，末块合成单个 OpenAI usage。
#[derive(Default)]
struct AnthropicTranscoder {
    lines: LineBuffer,
    id: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    stop_reason: Option<String>,
    role_sent: bool,
    /// Anthropic content block index → OpenAI tool_calls 数组下标（仅 tool_use blocks）
    tool_index_map: HashMap<i64, usize>,
    next_tool_index: usize,
    done_sent: bool,
}

impl SseTranscoder for AnthropicTranscoder {
    fn push(&mut self, upstream: &[u8]) -> Vec<u8> {
        if self.done_sent {
            return Vec::new(); // 已发错误/终止帧，丢弃后续上游字节
        }
        let lines = self.lines.take_lines(upstream);
        let mut out = Vec::new();
        for line in lines {
            let Some(data) = sse_data_payload(&line) else {
                continue;
            };
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            match ev.get("type").and_then(Value::as_str).unwrap_or("") {
                "message_start" => {
                    if let Some(msg) = ev.get("message") {
                        self.id = msg.get("id").and_then(Value::as_str).map(String::from);
                        let (i, o) = usage_tokens(msg.get("usage"));
                        self.input_tokens = i;
                        if o > 0 {
                            self.output_tokens = o;
                        }
                    }
                    if !self.role_sent {
                        self.role_sent = true;
                        out.extend(self.role_chunk());
                    }
                }
                "content_block_start" => {
                    let idx = ev.get("index").and_then(Value::as_i64).unwrap_or(0);
                    if let Some(cb) = ev.get("content_block") {
                        if cb.get("type").and_then(Value::as_str) == Some("tool_use") {
                            let oai_idx = self.next_tool_index;
                            self.next_tool_index += 1;
                            self.tool_index_map.insert(idx, oai_idx);
                            let id = cb.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = cb.get("name").and_then(Value::as_str).unwrap_or("");
                            out.extend(self.tool_start_chunk(oai_idx, id, name));
                        }
                    }
                }
                "content_block_delta" => {
                    let idx = ev.get("index").and_then(Value::as_i64).unwrap_or(0);
                    if let Some(delta) = ev.get("delta") {
                        match delta.get("type").and_then(Value::as_str) {
                            Some("text_delta") => {
                                if let Some(t) = delta.get("text").and_then(Value::as_str) {
                                    out.extend(self.text_chunk(t));
                                }
                            }
                            Some("input_json_delta") => {
                                if let (Some(pj), Some(&oai_idx)) = (
                                    delta.get("partial_json").and_then(Value::as_str),
                                    self.tool_index_map.get(&idx),
                                ) {
                                    out.extend(self.tool_args_chunk(oai_idx, pj));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "message_delta" => {
                    if let Some(sr) = ev
                        .get("delta")
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(Value::as_str)
                    {
                        self.stop_reason = Some(sr.to_string());
                    }
                    let (_, o) = usage_tokens(ev.get("usage"));
                    if o > 0 {
                        self.output_tokens = o;
                    }
                }
                // mid-stream 错误（Anthropic `event: error`）→ 转 OpenAI 错误帧并终止
                "error" => {
                    let msg = ev
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("upstream stream error");
                    let etype = ev
                        .get("error")
                        .and_then(|e| e.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("upstream_error");
                    out.extend(sse_frame(
                        &json!({"error": {"message": msg, "type": etype}}),
                    ));
                    self.done_sent = true;
                }
                // message_stop 不在此发末块：统一由 finish() 发（含 relay 流结束兜底，幂等）
                _ => {}
            }
            if self.done_sent {
                break; // 错误帧后停止处理后续事件
            }
        }
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.done_sent {
            return Vec::new();
        }
        self.done_sent = true;
        let total = self.input_tokens + self.output_tokens;
        let frame = json!({
            "id": self.id.clone().unwrap_or_default(),
            "object": "chat.completion.chunk",
            "created": 0,
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": map_stop_reason(self.stop_reason.as_deref()),
            }],
            "usage": {
                "prompt_tokens": self.input_tokens,
                "completion_tokens": self.output_tokens,
                "total_tokens": total,
            },
        });
        let mut out = sse_frame(&frame);
        out.extend(sse_done());
        out
    }
}

impl AnthropicTranscoder {
    fn role_chunk(&self) -> Vec<u8> {
        sse_frame(&self.chunk_with_delta(json!({"role": "assistant"})))
    }
    fn text_chunk(&self, text: &str) -> Vec<u8> {
        sse_frame(&self.chunk_with_delta(json!({"content": text})))
    }
    fn tool_start_chunk(&self, idx: usize, id: &str, name: &str) -> Vec<u8> {
        sse_frame(&self.chunk_with_delta(json!({
            "tool_calls": [{
                "index": idx, "id": id, "type": "function",
                "function": {"name": name, "arguments": ""},
            }]
        })))
    }
    fn tool_args_chunk(&self, idx: usize, args: &str) -> Vec<u8> {
        sse_frame(&self.chunk_with_delta(json!({
            "tool_calls": [{"index": idx, "function": {"arguments": args}}]
        })))
    }
    fn chunk_with_delta(&self, delta: Value) -> Value {
        json!({
            "id": self.id.clone().unwrap_or_default(),
            "object": "chat.completion.chunk",
            "created": 0,
            "choices": [{"index": 0, "delta": delta, "finish_reason": Value::Null}],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AdapterConfig<'static> {
        AdapterConfig::new(None)
    }

    #[test]
    fn auth_uses_x_api_key_not_bearer() {
        let h = AnthropicAdapter.auth_headers("sk-ant", &cfg());
        assert!(h.iter().any(|(k, v)| k == "x-api-key" && v == "sk-ant"));
        assert!(h.iter().any(|(k, _)| k == "anthropic-version"));
        // 绝不能出现 Authorization Bearer
        assert!(!h.iter().any(|(k, _)| k == "authorization"));
    }

    #[test]
    fn request_extracts_system_and_defaults_max_tokens() {
        let req = json!({
            "model": "claude",
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
            ],
        });
        let out = AnthropicAdapter.build_request_body(&req, "claude-3-5-sonnet", &cfg());
        assert_eq!(out["model"], json!("claude-3-5-sonnet"));
        assert_eq!(out["max_tokens"], json!(4096)); // 缺失 → 兜底
        assert_eq!(out["system"], json!("be brief"));
        // system 不进 messages
        assert_eq!(out["messages"].as_array().unwrap().len(), 1);
        assert_eq!(out["messages"][0]["role"], json!("user"));
        assert_eq!(out["messages"][0]["content"][0]["type"], json!("text"));
    }

    #[test]
    fn request_converts_image_and_tools() {
        let req = json!({
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "what is this"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
            ]}],
            "tools": [{"type": "function", "function": {
                "name": "get_weather", "description": "w",
                "parameters": {"type": "object", "properties": {}},
            }}],
            "max_tokens": 100,
        });
        let out = AnthropicAdapter.build_request_body(&req, "claude", &cfg());
        assert_eq!(out["max_tokens"], json!(100));
        let img = &out["messages"][0]["content"][1];
        assert_eq!(img["type"], json!("image"));
        assert_eq!(img["source"]["type"], json!("base64"));
        assert_eq!(img["source"]["media_type"], json!("image/png"));
        assert_eq!(img["source"]["data"], json!("AAAA"));
        // tools.function.parameters → input_schema
        assert_eq!(out["tools"][0]["name"], json!("get_weather"));
        assert_eq!(out["tools"][0]["input_schema"]["type"], json!("object"));
    }

    #[test]
    fn request_merges_consecutive_tool_results() {
        let req = json!({"messages": [
            {"role": "user", "content": "go"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}},
            ]},
            {"role": "tool", "tool_call_id": "c1", "content": "r1"},
            {"role": "tool", "tool_call_id": "c2", "content": "r2"},
        ]});
        let out = AnthropicAdapter.build_request_body(&req, "claude", &cfg());
        let msgs = out["messages"].as_array().unwrap();
        // user, assistant(tool_use), user(两个 tool_result 合并)
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1]["content"][0]["type"], json!("tool_use"));
        let results = msgs[2]["content"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["tool_use_id"], json!("c1"));
        assert_eq!(results[1]["tool_use_id"], json!("c2"));
    }

    #[test]
    fn response_maps_usage_and_tool_calls() {
        let upstream = json!({
            "id": "msg_1", "model": "claude",
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_use", "id": "tu1", "name": "f", "input": {"x": 1}},
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 12, "output_tokens": 7},
        });
        let out = AnthropicAdapter
            .convert_response(&upstream, &cfg())
            .unwrap();
        assert_eq!(out["object"], json!("chat.completion"));
        assert_eq!(out["choices"][0]["finish_reason"], json!("tool_calls"));
        assert_eq!(out["choices"][0]["message"]["content"], json!("hello"));
        let tc = &out["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], json!("f"));
        assert_eq!(tc["function"]["arguments"], json!("{\"x\":1}"));
        // 计费命门：usage 字段映射 + total
        assert_eq!(out["usage"]["prompt_tokens"], json!(12));
        assert_eq!(out["usage"]["completion_tokens"], json!(7));
        assert_eq!(out["usage"]["total_tokens"], json!(19));
    }

    #[test]
    fn error_normalized_to_openai_shape() {
        let body = br#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#;
        let out = AnthropicAdapter
            .convert_error(StatusCode::BAD_REQUEST, body, &cfg())
            .unwrap();
        assert_eq!(out["error"]["message"], json!("bad"));
        assert_eq!(out["error"]["type"], json!("invalid_request_error"));
        assert_eq!(out["error"]["code"], json!(400));
    }

    #[test]
    fn stream_transcodes_text_with_usage_finale() {
        let mut t = AnthropicTranscoder::default();
        let mut out = Vec::new();
        out.extend(t.push(b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_9\",\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n"));
        out.extend(t.push(b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n"));
        out.extend(t.push(b"event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":8}}\n\n"));
        out.extend(t.finish());
        let s = String::from_utf8(out).unwrap();
        // role 首块 + 携带上游 id（供 request_id 提取）
        assert!(s.contains("\"role\":\"assistant\""));
        assert!(s.contains("msg_9"));
        // 文本增量
        assert!(s.contains("\"content\":\"Hi\""));
        // 末块 usage（prompt=5, completion=8, total=13）+ finish_reason + DONE
        assert!(s.contains("\"prompt_tokens\":5"));
        assert!(s.contains("\"completion_tokens\":8"));
        assert!(s.contains("\"total_tokens\":13"));
        assert!(s.contains("\"finish_reason\":\"stop\""));
        assert!(s.trim_end().ends_with("data: [DONE]"));
    }

    #[test]
    fn stream_handles_split_chunk_boundary() {
        // 一帧被切成两次 push：转码器须靠行缓冲凑齐再解析
        let mut t = AnthropicTranscoder::default();
        let mut out = Vec::new();
        out.extend(t.push(b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text"));
        out.extend(t.push(b"_delta\",\"text\":\"yo\"}}\n\n"));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"content\":\"yo\""));
    }

    #[test]
    fn finish_is_idempotent() {
        let mut t = AnthropicTranscoder::default();
        let first = t.finish();
        assert!(!first.is_empty());
        assert!(t.finish().is_empty()); // 第二次（relay 流结束兜底）幂等
    }

    #[test]
    fn stream_mid_stream_error_emits_openai_error() {
        // 上游流中途 event: error → 转 OpenAI 错误帧，且不再发正常末块
        let mut t = AnthropicTranscoder::default();
        let out = t.push(
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"overloaded\"}}\n\n",
        );
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"error\""));
        assert!(s.contains("overloaded"));
        assert!(t.finish().is_empty()); // 错误后 finish 不再发末块/usage
    }
}
