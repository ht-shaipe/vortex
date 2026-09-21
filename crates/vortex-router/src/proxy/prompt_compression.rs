//! Prompt 压缩管线（opt-in）。
//!
//! 在路由前做去重、过滤 tool 输出、压缩重复 JSON、裁剪陈旧上下文。
//! fail-open 设计：压缩失败不阻塞请求。
//! Never-Worse 保护：压缩后若 token 估算更多则回退原始输入。

use serde_json::Value;

/// 压缩级别
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum CompressionLevel {
    /// 不压缩
    None,
    /// 最小压缩：去重 system + 裁剪 tool 输出 + 截断长 user/assistant content
    #[default]
    Minimal,
    /// 激进压缩：minimal + 裁剪陈旧上下文 + 裁剪所有超长 content
    Aggressive,
}


/// 压缩配置
#[derive(Clone, Debug)]
pub struct CompressionConfig {
    /// 是否启用
    pub enabled: bool,
    /// 压缩级别
    pub level: CompressionLevel,
    /// 最大上下文消息数
    pub max_messages: usize,
    /// 是否去重连续相同角色的消息
    pub dedup_consecutive: bool,
    /// 是否裁剪 tool 输出（只保留最后 N 行）
    pub trim_tool_output: bool,
    /// tool 输出保留行数
    pub tool_output_lines: usize,
    /// 激进模式下 content 字符串的最大字符数（超出截断）
    pub max_content_chars: usize,
    /// Minimal 模式下 user/assistant content 的截断阈值
    pub minimal_content_chars: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: CompressionLevel::Minimal,
            max_messages: 50,
            dedup_consecutive: true,
            trim_tool_output: true,
            tool_output_lines: 10,
            max_content_chars: 20000,
            minimal_content_chars: 8000,
        }
    }
}

/// 估算 JSON Value 的 token 数（粗略：每 4 字符 ≈ 1 token）
fn estimate_tokens(v: &Value) -> usize {
    let s = serde_json::to_string(v).unwrap_or_default();
    s.len() / 4
}

/// Never-Worse 保护：如果压缩后 token 更多，回退原始输入
fn never_worse(original: &Value, compressed: &Value) -> Value {
    if estimate_tokens(compressed) > estimate_tokens(original) {
        original.clone()
    } else {
        compressed.clone()
    }
}

/// 按字符边界安全截取字符串前 n 字节（避免 UTF-8 多字节字符中间截断 panic）。
fn take_bytes(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 按字符边界安全跳过前 skip 字节后返回剩余后缀（避免 UTF-8 多字节字符中间截断 panic）。
fn skip_bytes(s: &str, skip: usize) -> &str {
    if skip >= s.len() {
        return "";
    }
    let mut start = skip;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// 压缩消息列表，返回 (压缩后消息, 节省的 token 数)。
///
/// `messages` 兼容两种形态：
/// - 消息数组（OpenAI messages 数组本身）；
/// - 完整请求体对象（网关各端点实际传入的形态，如 Anthropic 入站请求转换后的
///   OpenAI body，其 `messages` 字段才是消息数组）。压缩后写回 `messages` 字段。
pub fn compress_messages(messages: &Value, config: &CompressionConfig) -> (Value, i64) {
    if !config.enabled || config.level == CompressionLevel::None {
        return (messages.clone(), 0);
    }

    // 形态一：消息数组
    if let Some(arr) = messages.as_array() {
        let (compressed, saved) = compress_array(arr, config);
        return (Value::Array(compressed), saved);
    }

    // 形态二：完整请求体对象（仅当含 "messages" 数组字段时压缩）
    if let Some(arr) = messages.get("messages").and_then(|m| m.as_array()) {
        let (compressed, saved) = compress_array(arr, config);
        if saved > 0 {
            let mut out = messages.clone();
            out["messages"] = Value::Array(compressed);
            return (out, saved);
        }
    }

    (messages.clone(), 0)
}

/// 压缩消息数组，返回 (压缩后消息数组, 节省的 token 数)
fn compress_array(arr: &[Value], config: &CompressionConfig) -> (Vec<Value>, i64) {
    let mut compressed: Vec<Value> = Vec::new();
    let mut last_role: Option<String> = None;

    for msg in arr {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();

        // 去重连续相同角色的消息
        if config.dedup_consecutive && config.level != CompressionLevel::None
            && Some(&role) == last_role.as_ref() && role == "system" {
                continue;
            }
        last_role = Some(role.clone());

        let mut msg = msg.clone();

        // 裁剪 tool 输出
        if config.trim_tool_output && role == "tool"
            && let Some(content) = msg.get_mut("content")
                && let Some(s) = content.as_str() {
                    let lines: Vec<&str> = s.lines().collect();
                    if lines.len() > config.tool_output_lines {
                        let trimmed = lines[lines.len() - config.tool_output_lines..].join("\n");
                        *content = Value::String(format!(
                            "[...truncated {} lines...]\n{}",
                            lines.len() - config.tool_output_lines,
                            trimmed
                        ));
                    }
                }

        // Minimal 模式：对长 user/assistant content 做头尾保留截断
        if config.level == CompressionLevel::Minimal
            && (role == "user" || role == "assistant")
            && let Some(content) = msg.get_mut("content")
                && let Some(s) = content.as_str()
                    && s.len() > config.minimal_content_chars {
                        let head = config.minimal_content_chars / 2;
                        let tail = config.minimal_content_chars / 4;
                        let truncated = format!(
                            "{}\n[...truncated {} chars...]\n{}",
                            take_bytes(s, head),
                            s.len() - head - tail,
                            skip_bytes(s, s.len() - tail)
                        );
                        *content = Value::String(truncated);
                    }

        // 激进模式：裁剪超长 content 字符串
        if config.level == CompressionLevel::Aggressive
            && let Some(content) = msg.get_mut("content")
                && let Some(s) = content.as_str()
                    && s.len() > config.max_content_chars {
                        let kept = take_bytes(s, config.max_content_chars);
                        *content = Value::String(format!(
                            "{}\n[...truncated {} chars...]",
                            kept,
                            s.len() - config.max_content_chars
                        ));
                    }

        compressed.push(msg);
    }

    // 裁剪陈旧上下文（minimal 和 aggressive 都执行）
    if compressed.len() > config.max_messages {
        let mut result = Vec::new();
        let mut non_system = Vec::new();
        for msg in &compressed {
            if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
                result.push(msg.clone());
            } else {
                non_system.push(msg.clone());
            }
        }
        let start = non_system.len().saturating_sub(config.max_messages);
        result.extend(non_system[start..].iter().cloned());
        compressed = result;
    }

    // Never-Worse 保护
    let original = Value::Array(arr.to_vec());
    let final_val = never_worse(&original, &Value::Array(compressed.clone()));
    let saved = (estimate_tokens(&original) as i64 - estimate_tokens(&final_val) as i64).max(0);

    let final_arr = match final_val {
        Value::Array(a) => a,
        _ => compressed,
    };
    (final_arr, saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(level: CompressionLevel) -> CompressionConfig {
        CompressionConfig {
            enabled: true,
            level,
            ..Default::default()
        }
    }

    /// 网关端点实际传入形态：完整请求体对象（含 "messages" 字段），应正常压缩并写回
    #[test]
    fn object_body_with_messages_array_is_compressed() {
        let long_tool_output = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let body = json!({
            "model": "gpt-4o",
            "max_tokens": 1024,
            "messages": [
                {"role": "system", "content": "you are helpful"},
                {"role": "user", "content": "run it"},
                {"role": "tool", "tool_call_id": "t1", "content": long_tool_output},
            ]
        });
        let (out, saved) = compress_messages(&body, &config(CompressionLevel::Minimal));
        assert!(saved > 0, "saved should be > 0, got {}", saved);
        assert_eq!(out["model"], "gpt-4o", "请求体其它字段应保留");
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        let tool_content = msgs[2]["content"].as_str().unwrap();
        assert!(tool_content.starts_with("[...truncated 40 lines...]\n"), "tool 输出应裁剪为最后 10 行");
    }

    /// 纯消息数组形态（旧行为）仍应支持
    #[test]
    fn array_input_still_supported() {
        let long_tool_output = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let arr = json!([
            {"role": "tool", "tool_call_id": "t1", "content": long_tool_output},
        ]);
        let (out, saved) = compress_messages(&arr, &config(CompressionLevel::Minimal));
        assert!(saved > 0);
        assert!(out.as_array().unwrap()[0]["content"].as_str().unwrap().contains("[...truncated"));
    }

    /// 中文长文本截断：UTF-8 多字节字符边界处不应 panic
    #[test]
    fn minimal_truncate_handles_utf8_boundaries() {
        let long_cn = "汉".repeat(5000); // 15000 字节 > minimal_content_chars(8000)
        let arr = json!([
            {"role": "user", "content": long_cn},
        ]);
        let (out, saved) = compress_messages(&arr, &config(CompressionLevel::Minimal));
        assert!(saved > 0);
        let s = out.as_array().unwrap()[0]["content"].as_str().unwrap();
        assert!(s.contains("[...truncated"));
    }

    /// 无 messages 数组的对象、未启用、level=None 均应原样返回 saved=0
    #[test]
    fn passthrough_cases() {
        let no_messages = json!({"input": "hello"});
        let (out, saved) = compress_messages(&no_messages, &config(CompressionLevel::Minimal));
        assert_eq!(saved, 0);
        assert_eq!(out, no_messages);

        let body = json!({"messages": [{"role": "user", "content": "hi"}]});
        let mut disabled = config(CompressionLevel::Minimal);
        disabled.enabled = false;
        let (_, saved) = compress_messages(&body, &disabled);
        assert_eq!(saved, 0);

        let (_, saved) = compress_messages(&body, &config(CompressionLevel::None));
        assert_eq!(saved, 0);
    }

    /// 消息数超上限时裁剪陈旧上下文（保留 system + 最近的 max_messages 条）
    #[test]
    fn stale_context_trimmed() {
        let mut msgs = vec![json!({"role": "system", "content": "sys"})];
        for i in 0..80 {
            msgs.push(json!({"role": "user", "content": format!("msg {}", i)}));
        }
        let body = json!({"messages": msgs});
        let (out, saved) = compress_messages(&body, &config(CompressionLevel::Minimal));
        assert!(saved > 0);
        let kept = out["messages"].as_array().unwrap();
        assert_eq!(kept.len(), 1 + 50, "应保留 1 条 system + 最近 50 条");
        assert_eq!(kept[0]["role"], "system");
        assert_eq!(kept[1]["content"], "msg 30", "旧的 user 消息应被裁掉");
    }
}
