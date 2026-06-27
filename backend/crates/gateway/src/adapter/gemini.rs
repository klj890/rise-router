//! Gemini `generateContent` 协议适配器：OpenAI chat ↔ Gemini 双向转换
//! （含 tools/function calling、多模态 image、错误归一、流式 SSE）。
//!
//! quirk：模型名在 URL path（`:generateContent` / `:streamGenerateContent`），鉴权用
//! `x-goog-api-key` 头；流式 `alt=sse` 已 `data:{json}` 分帧但**不发 `[DONE]`**，由 finish() 补。

use std::collections::HashMap;

use axum::http::{HeaderName, HeaderValue, StatusCode};
use serde_json::{json, Map, Value};

use super::{
    sse_data_payload, sse_done, sse_frame, AdapterConfig, LineBuffer, ProtocolAdapter,
    SseTranscoder,
};

pub struct GeminiAdapter;

impl ProtocolAdapter for GeminiAdapter {
    fn request_url(
        &self,
        base_url: &str,
        upstream_model: &str,
        is_stream: bool,
        cfg: &AdapterConfig,
    ) -> String {
        let action = if is_stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let base = base_url.trim_end_matches('/');
        // path_template 可覆盖（模型名 / action 占位符）；默认 v1beta
        let path = cfg
            .path_template()
            .unwrap_or("/v1beta/models/{model}:{action}")
            .replace("{model}", upstream_model)
            .replace("{action}", action);
        let mut url = format!("{base}{path}");
        if is_stream {
            url.push_str(if url.contains('?') {
                "&alt=sse"
            } else {
                "?alt=sse"
            });
        }
        url
    }

    fn auth_headers(&self, key: &str, _cfg: &AdapterConfig) -> Vec<(HeaderName, HeaderValue)> {
        // header 鉴权（不把 key 拼进 URL，避免进日志）
        match HeaderValue::from_str(key) {
            Ok(v) => vec![(HeaderName::from_static("x-goog-api-key"), v)],
            Err(_) => Vec::new(),
        }
    }

    fn build_request_body(
        &self,
        openai_req: &Value,
        _upstream_model: &str,
        _cfg: &AdapterConfig,
    ) -> Value {
        // 模型名在 URL path，body 不带 model
        let mut out = Map::new();

        let (contents, system) = convert_contents(openai_req.get("messages"));
        out.insert("contents".into(), Value::Array(contents));
        if let Some(sys) = system {
            out.insert("systemInstruction".into(), sys);
        }

        if let Some(tools) = openai_req.get("tools").and_then(Value::as_array) {
            let decls = function_declarations(tools);
            if !decls.is_empty() {
                out.insert(
                    "tools".into(),
                    json!([{"functionDeclarations": Value::Array(decls)}]),
                );
            }
        }

        let gc = generation_config(openai_req);
        if !gc.is_empty() {
            out.insert("generationConfig".into(), Value::Object(gc));
        }

        Value::Object(out)
    }

    fn convert_response(&self, upstream: &Value, _cfg: &AdapterConfig) -> Option<Value> {
        let cand = upstream
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|a| a.first());

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish = "stop";
        if let Some(cand) = cand {
            if let Some(parts) = cand
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(Value::as_array)
            {
                for p in parts {
                    if let Some(t) = p.get("text").and_then(Value::as_str) {
                        text.push_str(t);
                    } else if let Some(fc) = p.get("functionCall") {
                        tool_calls.push(function_call_to_openai(fc, tool_calls.len()));
                    }
                }
            }
            finish = map_finish_reason(cand.get("finishReason").and_then(Value::as_str));
        }

        let (p, c, t) = usage_metadata(upstream.get("usageMetadata"));
        let model = upstream
            .get("modelVersion")
            .and_then(Value::as_str)
            .unwrap_or("");

        let mut message = Map::new();
        message.insert("role".into(), json!("assistant"));
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
            "id": "",
            "object": "chat.completion",
            "created": 0,
            "model": model,
            "choices": [{"index": 0, "message": Value::Object(message), "finish_reason": finish}],
            "usage": {"prompt_tokens": p, "completion_tokens": c, "total_tokens": t},
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
                    .and_then(|e| e.get("status"))
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
        Box::new(GeminiTranscoder::default())
    }
}

// ─── 请求转换 helper ────────────────────────────────────────────────────────

/// OpenAI messages → (Gemini contents, systemInstruction)。
fn convert_contents(messages: Option<&Value>) -> (Vec<Value>, Option<Value>) {
    let mut contents: Vec<Value> = Vec::new();
    let mut system_parts: Vec<String> = Vec::new();
    // tool_call_id → 函数名（OpenAI tool message 只带 id，Gemini functionResponse 需要 name）
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let Some(arr) = messages.and_then(Value::as_array) else {
        return (contents, None);
    };
    for msg in arr {
        match msg.get("role").and_then(Value::as_str).unwrap_or("") {
            "system" | "developer" => {
                if let Some(s) = content_to_text(msg.get("content")) {
                    system_parts.push(s);
                }
            }
            "user" => {
                contents.push(json!({"role": "user", "parts": convert_parts(msg.get("content"))}))
            }
            "assistant" => {
                let mut parts = convert_parts(msg.get("content"));
                if let Some(tcs) = msg.get("tool_calls").and_then(Value::as_array) {
                    for tc in tcs {
                        if let (Some(id), Some(name)) = (
                            tc.get("id").and_then(Value::as_str),
                            tc.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(Value::as_str),
                        ) {
                            tool_names.insert(id.to_string(), name.to_string());
                        }
                        parts.push(tool_call_to_function_call(tc));
                    }
                }
                contents.push(json!({"role": "model", "parts": parts}));
            }
            "tool" => {
                let call_id = msg
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let name = msg
                    .get("name")
                    .and_then(Value::as_str)
                    .map(String::from)
                    .or_else(|| tool_names.get(call_id).cloned())
                    .unwrap_or_default();
                let part = json!({
                    "functionResponse": {
                        "name": name,
                        "response": {"content": content_to_text(msg.get("content")).unwrap_or_default()},
                    }
                });
                if append_to_function_response(&mut contents, &part) {
                    continue;
                }
                contents.push(json!({"role": "user", "parts": [part]}));
            }
            _ => {}
        }
    }
    let system =
        (!system_parts.is_empty()).then(|| json!({"parts": [{"text": system_parts.join("\n\n")}]}));
    (contents, system)
}

/// 连续 functionResponse 合并进同一 user content（Gemini 要求 role 交替）。
fn append_to_function_response(contents: &mut [Value], part: &Value) -> bool {
    let Some(last) = contents.last_mut() else {
        return false;
    };
    if last.get("role").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let is_fr_agg = last
        .get("parts")
        .and_then(Value::as_array)
        .and_then(|a| a.last())
        .map(|p| p.get("functionResponse").is_some())
        .unwrap_or(false);
    if !is_fr_agg {
        return false;
    }
    if let Some(arr) = last.get_mut("parts").and_then(Value::as_array_mut) {
        arr.push(part.clone());
        return true;
    }
    false
}

/// content（string | array）→ Gemini parts（text + inlineData/fileData）。
fn convert_parts(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(s)) if !s.is_empty() => vec![json!({"text": s})],
        Some(Value::Array(arr)) => {
            let mut parts = Vec::new();
            for part in arr {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            parts.push(json!({"text": t}));
                        }
                    }
                    Some("image_url") => {
                        if let Some(url) = part
                            .get("image_url")
                            .and_then(|u| u.get("url"))
                            .and_then(Value::as_str)
                        {
                            parts.push(image_part(url));
                        }
                    }
                    _ => {}
                }
            }
            parts
        }
        _ => vec![],
    }
}

/// OpenAI image_url → Gemini inlineData(base64) / fileData(url)。
fn image_part(url: &str) -> Value {
    if let Some(rest) = url.strip_prefix("data:") {
        if let Some((meta, data)) = rest.split_once(',') {
            let mime = meta.split(';').next().unwrap_or("image/png");
            return json!({"inlineData": {"mimeType": mime, "data": data}});
        }
    }
    json!({"fileData": {"fileUri": url}})
}

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

fn function_declarations(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            let name = f.get("name").and_then(Value::as_str)?;
            let mut o = Map::new();
            o.insert("name".into(), json!(name));
            if let Some(d) = f.get("description") {
                o.insert("description".into(), d.clone());
            }
            if let Some(params) = f.get("parameters") {
                o.insert("parameters".into(), params.clone());
            }
            Some(Value::Object(o))
        })
        .collect()
}

/// OpenAI assistant tool_call → Gemini functionCall part。
fn tool_call_to_function_call(tc: &Value) -> Value {
    let f = tc.get("function");
    let args = f
        .and_then(|f| f.get("arguments"))
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(|| json!({}));
    json!({
        "functionCall": {
            "name": f.and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or(""),
            "args": args,
        }
    })
}

/// Gemini functionCall → OpenAI tool_call（Gemini 无 id，按序生成 `call_{idx}`）。
fn function_call_to_openai(fc: &Value, idx: usize) -> Value {
    let args = fc.get("args").cloned().unwrap_or_else(|| json!({}));
    json!({
        "id": format!("call_{idx}"),
        "type": "function",
        "function": {
            "name": fc.get("name").and_then(Value::as_str).unwrap_or(""),
            "arguments": serde_json::to_string(&args).unwrap_or_else(|_| "{}".into()),
        },
    })
}

fn generation_config(req: &Value) -> Map<String, Value> {
    let mut gc = Map::new();
    if let Some(t) = req.get("temperature") {
        gc.insert("temperature".into(), t.clone());
    }
    if let Some(p) = req.get("top_p") {
        gc.insert("topP".into(), p.clone());
    }
    if let Some(k) = req.get("top_k") {
        gc.insert("topK".into(), k.clone());
    }
    if let Some(m) = req
        .get("max_tokens")
        .or_else(|| req.get("max_completion_tokens"))
    {
        gc.insert("maxOutputTokens".into(), m.clone());
    }
    if let Some(stop) = req.get("stop") {
        let seqs = match stop {
            Value::String(s) => vec![Value::String(s.clone())],
            Value::Array(a) => a.clone(),
            _ => vec![],
        };
        if !seqs.is_empty() {
            gc.insert("stopSequences".into(), Value::Array(seqs));
        }
    }
    gc
}

fn usage_metadata(um: Option<&Value>) -> (i64, i64, i64) {
    let p = um
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let c = um
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let t = um
        .and_then(|u| u.get("totalTokenCount"))
        .and_then(Value::as_i64)
        .unwrap_or(p + c);
    (p, c, t)
}

/// Gemini finishReason → OpenAI finish_reason。
fn map_finish_reason(fr: Option<&str>) -> &'static str {
    match fr {
        Some("MAX_TOKENS") => "length",
        Some("SAFETY") | Some("RECITATION") | Some("PROHIBITED_CONTENT") | Some("BLOCKLIST") => {
            "content_filter"
        }
        _ => "stop",
    }
}

// ─── 流式 SSE 转码器 ─────────────────────────────────────────────────────────

/// Gemini `alt=sse`（每帧 `data:{json}`，不发 `[DONE]`）→ OpenAI chunk 流。
#[derive(Default)]
struct GeminiTranscoder {
    lines: LineBuffer,
    role_sent: bool,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    finish_reason: Option<String>,
    next_tool_index: usize,
    done_sent: bool,
}

impl SseTranscoder for GeminiTranscoder {
    fn push(&mut self, upstream: &[u8]) -> Vec<u8> {
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
            if !self.role_sent {
                self.role_sent = true;
                out.extend(self.role_chunk());
            }
            if let Some(cand) = ev
                .get("candidates")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
            {
                if let Some(parts) = cand
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(Value::as_array)
                {
                    for p in parts {
                        if let Some(t) = p.get("text").and_then(Value::as_str) {
                            out.extend(self.text_chunk(t));
                        } else if let Some(fc) = p.get("functionCall") {
                            let idx = self.next_tool_index;
                            self.next_tool_index += 1;
                            out.extend(self.tool_chunk(idx, fc));
                        }
                    }
                }
                if let Some(fr) = cand.get("finishReason").and_then(Value::as_str) {
                    self.finish_reason = Some(fr.to_string());
                }
            }
            if let Some(um) = ev.get("usageMetadata") {
                let (p, c, t) = usage_metadata(Some(um));
                self.prompt_tokens = p;
                self.completion_tokens = c;
                self.total_tokens = t;
            }
        }
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.done_sent {
            return Vec::new();
        }
        self.done_sent = true;
        let total = if self.total_tokens > 0 {
            self.total_tokens
        } else {
            self.prompt_tokens + self.completion_tokens
        };
        let frame = json!({
            "id": "",
            "object": "chat.completion.chunk",
            "created": 0,
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": map_finish_reason(self.finish_reason.as_deref()),
            }],
            "usage": {
                "prompt_tokens": self.prompt_tokens,
                "completion_tokens": self.completion_tokens,
                "total_tokens": total,
            },
        });
        let mut out = sse_frame(&frame);
        out.extend(sse_done());
        out
    }
}

impl GeminiTranscoder {
    fn role_chunk(&self) -> Vec<u8> {
        sse_frame(&chunk_with_delta(json!({"role": "assistant"})))
    }
    fn text_chunk(&self, text: &str) -> Vec<u8> {
        sse_frame(&chunk_with_delta(json!({"content": text})))
    }
    fn tool_chunk(&self, idx: usize, fc: &Value) -> Vec<u8> {
        let args = fc.get("args").cloned().unwrap_or_else(|| json!({}));
        sse_frame(&chunk_with_delta(json!({
            "tool_calls": [{
                "index": idx,
                "id": format!("call_{idx}"),
                "type": "function",
                "function": {
                    "name": fc.get("name").and_then(Value::as_str).unwrap_or(""),
                    "arguments": serde_json::to_string(&args).unwrap_or_else(|_| "{}".into()),
                },
            }]
        })))
    }
}

fn chunk_with_delta(delta: Value) -> Value {
    json!({
        "id": "",
        "object": "chat.completion.chunk",
        "created": 0,
        "choices": [{"index": 0, "delta": delta, "finish_reason": Value::Null}],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AdapterConfig<'static> {
        AdapterConfig::new(None)
    }

    #[test]
    fn url_embeds_model_and_stream_action() {
        let a = GeminiAdapter;
        assert_eq!(
            a.request_url("https://x/", "gemini-2.0-flash", false, &cfg()),
            "https://x/v1beta/models/gemini-2.0-flash:generateContent"
        );
        let s = a.request_url("https://x", "gemini-2.0-flash", true, &cfg());
        assert!(s.contains(":streamGenerateContent"));
        assert!(s.ends_with("?alt=sse"));
    }

    #[test]
    fn auth_uses_goog_api_key() {
        let h = GeminiAdapter.auth_headers("k", &cfg());
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0, "x-goog-api-key");
        assert!(!h.iter().any(|(k, _)| k == "authorization"));
    }

    #[test]
    fn request_maps_roles_system_image_tools() {
        let req = json!({
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": [
                    {"type": "text", "text": "look"},
                    {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,ZZ"}},
                ]},
                {"role": "assistant", "content": "ok"},
            ],
            "tools": [{"type": "function", "function": {"name": "f", "parameters": {"type": "object"}}}],
            "temperature": 0.5,
            "max_tokens": 256,
        });
        let out = GeminiAdapter.build_request_body(&req, "gemini", &cfg());
        // system → systemInstruction（不进 contents）
        assert_eq!(out["systemInstruction"]["parts"][0]["text"], json!("sys"));
        assert_eq!(out["contents"].as_array().unwrap().len(), 2);
        // assistant → role: model
        assert_eq!(out["contents"][1]["role"], json!("model"));
        // image → inlineData
        assert_eq!(
            out["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            json!("image/jpeg")
        );
        assert_eq!(
            out["contents"][0]["parts"][1]["inlineData"]["data"],
            json!("ZZ")
        );
        // tools → functionDeclarations
        assert_eq!(
            out["tools"][0]["functionDeclarations"][0]["name"],
            json!("f")
        );
        // generationConfig 改名
        assert_eq!(out["generationConfig"]["temperature"], json!(0.5));
        assert_eq!(out["generationConfig"]["maxOutputTokens"], json!(256));
    }

    #[test]
    fn request_tool_result_gets_name_from_prior_call() {
        let req = json!({"messages": [
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":1}"}},
            ]},
            {"role": "tool", "tool_call_id": "c1", "content": "result"},
        ]});
        let out = GeminiAdapter.build_request_body(&req, "gemini", &cfg());
        // assistant → functionCall part
        assert_eq!(out["contents"][0]["role"], json!("model"));
        assert_eq!(
            out["contents"][0]["parts"][0]["functionCall"]["name"],
            json!("lookup")
        );
        // tool → functionResponse，name 从前序 tool_call 回填
        let fr = &out["contents"][1]["parts"][0]["functionResponse"];
        assert_eq!(fr["name"], json!("lookup"));
        assert_eq!(fr["response"]["content"], json!("result"));
    }

    #[test]
    fn response_maps_usage_and_function_call() {
        let upstream = json!({
            "candidates": [{
                "content": {"role": "model", "parts": [
                    {"text": "hi"},
                    {"functionCall": {"name": "f", "args": {"x": 1}}},
                ]},
                "finishReason": "STOP",
            }],
            "usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 4, "totalTokenCount": 13},
        });
        let out = GeminiAdapter.convert_response(&upstream, &cfg()).unwrap();
        assert_eq!(out["choices"][0]["finish_reason"], json!("stop"));
        assert_eq!(out["choices"][0]["message"]["content"], json!("hi"));
        let tc = &out["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["id"], json!("call_0"));
        assert_eq!(tc["function"]["name"], json!("f"));
        assert_eq!(tc["function"]["arguments"], json!("{\"x\":1}"));
        assert_eq!(out["usage"]["prompt_tokens"], json!(9));
        assert_eq!(out["usage"]["completion_tokens"], json!(4));
        assert_eq!(out["usage"]["total_tokens"], json!(13));
    }

    #[test]
    fn safety_finish_maps_to_content_filter() {
        let upstream =
            json!({"candidates": [{"content": {"parts": []}, "finishReason": "SAFETY"}]});
        let out = GeminiAdapter.convert_response(&upstream, &cfg()).unwrap();
        assert_eq!(out["choices"][0]["finish_reason"], json!("content_filter"));
    }

    #[test]
    fn error_normalized() {
        let body = br#"{"error":{"code":400,"message":"bad key","status":"INVALID_ARGUMENT"}}"#;
        let out = GeminiAdapter
            .convert_error(StatusCode::BAD_REQUEST, body, &cfg())
            .unwrap();
        assert_eq!(out["error"]["message"], json!("bad key"));
        assert_eq!(out["error"]["type"], json!("INVALID_ARGUMENT"));
        assert_eq!(out["error"]["code"], json!(400));
    }

    #[test]
    fn stream_transcodes_text_and_appends_done() {
        let mut t = GeminiTranscoder::default();
        let mut out = Vec::new();
        out.extend(
            t.push(b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]}}]}\n\n"),
        );
        out.extend(t.push(b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"!\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":2,\"totalTokenCount\":5}}\n\n"));
        out.extend(t.finish());
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\"role\":\"assistant\""));
        assert!(s.contains("\"content\":\"Hi\""));
        assert!(s.contains("\"content\":\"!\""));
        // usage 末块 + finish + Gemini 不发 DONE → finish() 补
        assert!(s.contains("\"prompt_tokens\":3"));
        assert!(s.contains("\"completion_tokens\":2"));
        assert!(s.contains("\"total_tokens\":5"));
        assert!(s.contains("\"finish_reason\":\"stop\""));
        assert!(s.trim_end().ends_with("data: [DONE]"));
    }
}
