//! SSE / JSON 响应提取工具（平台无关部分）：
//! SSE 事件行解析、AiReply 去重合并、content block 文本提取、深度文本搜索

use crate::models::{AiReply, Reference};

/// 从 JSON Value 中深度搜索文本内容字段。
/// 尝试常见字段名：content, text, message, answer, reply, delta, output
pub(crate) fn json_find_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => {
            if s.len() > 2 && !s.starts_with("data:") {
                return Some(s.clone());
            }
            None
        }
        serde_json::Value::Object(o) => {
            // 优先检查已知文本字段
            for key in &[
                "content", "text", "message", "answer", "reply", "output", "delta_text",
                "completion", "result",
            ] {
                if let Some(v2) = o.get(*key) {
                    if let Some(s) = v2.as_str() {
                        if s.len() > 2 {
                            return Some(s.to_string());
                        }
                    }
                    if let Some(s) = json_find_text(v2) {
                        return Some(s);
                    }
                }
            }
            // 递归搜索（限深度 3 层避免性能问题）
            for (_, v2) in o {
                if let Some(s) = json_find_text(v2) {
                    return Some(s);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for v2 in arr {
                if let Some(s) = json_find_text(v2) {
                    return Some(s);
                }
            }
            None
        }
        _ => None,
    }
}

/// 从 content 的 *_block 中提取 text 字段（text_block 优先，其余 thinking_block 等通用匹配）
pub(crate) fn block_text_value(content: &serde_json::Value) -> Option<String> {
    let obj = content.as_object()?;
    if let Some(t) = obj
        .get("text_block")
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
    {
        return Some(t.to_string());
    }
    for (k, v) in obj {
        if k.ends_with("_block") {
            if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

/// 解析 SSE 响应体为 (event, data) 序列。
/// 空行分隔事件；EOF 时仅当 pending event 非空才补发（与流式解析语义一致）。
pub(crate) fn parse_sse_events(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current_event = String::new();
    let mut current_data = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            out.push((current_event.clone(), current_data.clone()));
            current_event.clear();
            current_data.clear();
        } else if let Some(e) = line.strip_prefix("event:") {
            current_event = e.trim().to_string();
        } else if let Some(d) = line.strip_prefix("data:") {
            current_data = d.trim().to_string();
        }
    }
    if !current_event.is_empty() {
        out.push((current_event, current_data));
    }
    out
}

/// 合并新提取的 AiReply 到累计结果（思考/答案文本 前缀替换或追加；关键字/引用去重）
pub(crate) fn merge_ai(reply: &mut AiReply, new: AiReply) {
    let merge_text = |dst: &mut String, src: String| {
        if src.starts_with(dst.as_str()) && src.len() > dst.len() {
            // 新文本包含旧文本且更长 → 完整替换
            *dst = src;
        } else if !dst.is_empty() && !src.is_empty() {
            // 增量文本 → 追加
            dst.push_str(&src);
        } else if dst.is_empty() {
            *dst = src;
        }
    };
    merge_text(&mut reply.thinking, new.thinking);
    merge_text(&mut reply.reply, new.reply);
    // 搜索关键字与参考资料去重合并
    for q in new.search_queries {
        if !reply.search_queries.contains(&q) {
            reply.search_queries.push(q);
        }
    }
    for r in new.references {
        if !reply.references.iter().any(|e: &Reference| e.url == r.url) {
            reply.references.push(r);
        }
    }
}
