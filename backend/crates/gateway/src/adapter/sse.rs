//! SSE 共享工具：跨 chunk 行缓冲 + OpenAI SSE 帧构造。
//!
//! 上游 SSE 帧会跨 TCP chunk 切断，转码器不能假设一次 `push` 收到完整帧 → 用 [`LineBuffer`]
//! 缓冲半行。算法复用 relay `UsageScanner` 的成熟做法：1MB 上限防 OOM、consumed 游标 +
//! 末尾单次 drain 防 O(N²) 搬移。

use serde_json::Value;

/// 跨 chunk 行缓冲器：喂字节，吐已完成的整行（不含 `\n`），保留不完整尾行待下次。
#[derive(Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
}

impl LineBuffer {
    /// 喂入 `bytes`，返回本次新凑齐的完整行（owned）。非法 UTF-8 行跳过。
    pub fn take_lines(&mut self, bytes: &[u8]) -> Vec<String> {
        // 防 DoS：上游若发无换行的超长流，buf 会无限增长 → 1MB 封顶（容纳大型 tool call JSON）。
        const MAX_BUF: usize = 1024 * 1024;
        let mut out = Vec::new();
        if self.buf.len() + bytes.len() > MAX_BUF {
            self.buf.clear();
            if bytes.len() > MAX_BUF {
                return out;
            }
        }
        self.buf.extend_from_slice(bytes);
        // '\n' 是 ASCII，按字节切行对 UTF-8 安全。consumed 游标 + 末尾单次 drain 防 O(N²)。
        let mut consumed = 0;
        while let Some(pos) = self.buf[consumed..].iter().position(|&b| b == b'\n') {
            let end = consumed + pos;
            if let Ok(s) = std::str::from_utf8(&self.buf[consumed..end]) {
                out.push(s.to_string());
            }
            consumed = end + 1;
        }
        if consumed > 0 {
            self.buf.drain(..consumed);
        }
        out
    }
}

/// 从一行 SSE 里取 `data:` 负载（已 trim）。非 data 行返回 None。
pub fn sse_data_payload(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("data:").map(str::trim)
}

/// 构造一帧 OpenAI SSE：`data: {json}\n\n`。
pub fn sse_frame(json: &Value) -> Vec<u8> {
    format!("data: {json}\n\n").into_bytes()
}

/// 流终止帧：`data: [DONE]\n\n`。
pub fn sse_done() -> Vec<u8> {
    b"data: [DONE]\n\n".to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffers_across_chunk_boundaries() {
        let mut lb = LineBuffer::default();
        // 帧被切成两段，第一段无换行 → 不产出
        assert!(lb.take_lines(b"data: {\"a\"").is_empty());
        // 补齐后产出一整行（不含 '\n'）
        let lines = lb.take_lines(b":1}\nhalf");
        assert_eq!(lines, vec!["data: {\"a\":1}".to_string()]);
        // "half" 仍滞留，下次补全
        let lines = lb.take_lines(b"-line\n");
        assert_eq!(lines, vec!["half-line".to_string()]);
    }

    #[test]
    fn extracts_data_payload() {
        assert_eq!(sse_data_payload("data: {\"x\":1}"), Some("{\"x\":1}"));
        assert_eq!(sse_data_payload("data:[DONE]"), Some("[DONE]"));
        assert_eq!(sse_data_payload("event: message_start"), None);
        assert_eq!(sse_data_payload(""), None);
    }

    #[test]
    fn frame_and_done_format() {
        let f = sse_frame(&serde_json::json!({"x": 1}));
        assert_eq!(f, b"data: {\"x\":1}\n\n".to_vec());
        assert_eq!(sse_done(), b"data: [DONE]\n\n".to_vec());
    }
}
