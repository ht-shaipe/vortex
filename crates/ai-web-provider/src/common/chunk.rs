//! 结构化 chunk 事件：流式模式下 worker → HTTP 层的增量协议。
//!
//! worker 通过 `chunk_tx: mpsc::Sender<String>` 下发增量，字符串通道里用
//! 紧凑 JSON 事件区分思考与正文：
//!
//! ```text
//! {"t":"thinking","c":"思考增量"}
//! {"t":"text","c":"正文增量"}
//! ```
//!
//! - 未编码的纯文本 chunk（如 deepseek 等旧实现）原样透传，解码返回 None。
//! - 旧版 `/api/{platform}/ask` SSE 输出只取事件文本部分，保持对外行为不变。
//! - OpenAI / Anthropic 兼容层（crate::compat）按事件类型分流：
//!   thinking → reasoning_content / thinking 块，text → content / text 块。

/// chunk 事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    /// 思考过程增量
    Thinking,
    /// 正文增量
    Text,
}

impl ChunkKind {
    fn as_str(self) -> &'static str {
        match self {
            ChunkKind::Thinking => "thinking",
            ChunkKind::Text => "text",
        }
    }
}

/// 编码一条结构化 chunk 事件为字符串（走 chunk_tx 通道）。
pub fn encode_chunk(kind: ChunkKind, text: &str) -> String {
    serde_json::json!({ "t": kind.as_str(), "c": text }).to_string()
}

/// 解码 chunk 字符串。
///
/// - 结构化事件（`{"t":..,"c":..}`）→ `Some((类型, 文本))`
/// - 纯文本 chunk（未编码的旧实现）→ `None`（调用方按正文原样处理）
pub fn decode_chunk(s: &str) -> Option<(ChunkKind, String)> {
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    if !v.is_object() {
        return None;
    }
    // 必须严格是 {"t":..,"c":..} 结构，避免把恰好是 JSON 的普通文本误判为事件
    if v.as_object().map(|m| m.len()) != Some(2) {
        return None;
    }
    let kind = match v.get("t")?.as_str()? {
        "thinking" => ChunkKind::Thinking,
        "text" => ChunkKind::Text,
        _ => return None,
    };
    let text = v.get("c")?.as_str()?.to_string();
    Some((kind, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for (kind, k) in [
            (ChunkKind::Thinking, "thinking"),
            (ChunkKind::Text, "text"),
        ] {
            let s = encode_chunk(kind, "你好\n\"引号\"");
            assert_eq!(s, format!("{{\"t\":\"{k}\",\"c\":\"你好\\n\\\"引号\\\"\"}}"));
            let (d, t) = decode_chunk(&s).unwrap();
            assert_eq!(d, kind);
            assert_eq!(t, "你好\n\"引号\"");
        }
    }

    #[test]
    fn plain_text_not_decoded() {
        assert_eq!(decode_chunk("普通文本"), None);
        assert_eq!(decode_chunk("{\"content\":\"x\"}"), None);
        assert_eq!(decode_chunk(""), None);
        // 缺 c 字段
        assert_eq!(decode_chunk("{\"t\":\"text\"}"), None);
    }
}
