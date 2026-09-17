//! Prompt 压缩管线（opt-in）。
//!
//! 在路由前做去重、过滤 tool 输出、压缩重复 JSON、裁剪陈旧上下文。
//! fail-open 设计：压缩失败不阻塞请求。
//! Never-Worse 保护：压缩后若 token 估算更多则回退原始输入。

use serde_json::Value;

/// 压缩级别
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CompressionLevel {
    /// 不压缩
    None,
    /// 最小压缩：去重 system + 裁剪 tool 输出 + 截断长 user/assistant content
    Minimal,
    /// 激进压缩：minimal + 裁剪陈旧上下文 + 裁剪所有超长 content
    Aggressive,
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::Minimal
    }
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

/// 压缩消息列表，返回 (压缩后消息, 节省的 token 数)
pub fn compress_messages(messages: &Value, config: &CompressionConfig) -> (Value, i64) {
    if !config.enabled || config.level == CompressionLevel::None {
        return (messages.clone(), 0);
    }

    let arr = match messages.as_array() {
        Some(a) => a,
        None => return (messages.clone(), 0),
    };

    let mut compressed: Vec<Value> = Vec::new();
    let mut last_role: Option<String> = None;

    for msg in arr {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();

        // 去重连续相同角色的消息
        if config.dedup_consecutive && config.level != CompressionLevel::None {
            if Some(&role) == last_role.as_ref() && role == "system" {
                continue;
            }
        }
        last_role = Some(role.clone());

        let mut msg = msg.clone();

        // 裁剪 tool 输出
        if config.trim_tool_output && role == "tool" {
            if let Some(content) = msg.get_mut("content") {
                if let Some(s) = content.as_str() {
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
            }
        }

        // Minimal 模式：对长 user/assistant content 做头尾保留截断
        if config.level == CompressionLevel::Minimal
            && (role == "user" || role == "assistant")
        {
            if let Some(content) = msg.get_mut("content") {
                if let Some(s) = content.as_str() {
                    if s.len() > config.minimal_content_chars {
                        let head = config.minimal_content_chars / 2;
                        let tail = config.minimal_content_chars / 4;
                        let truncated = format!(
                            "{}\n[...truncated {} chars...]\n{}",
                            &s[..head],
                            s.len() - head - tail,
                            &s[s.len() - tail..]
                        );
                        *content = Value::String(truncated);
                    }
                }
            }
        }

        // 激进模式：裁剪超长 content 字符串
        if config.level == CompressionLevel::Aggressive {
            if let Some(content) = msg.get_mut("content") {
                if let Some(s) = content.as_str() {
                    if s.len() > config.max_content_chars {
                        let kept = &s[..config.max_content_chars];
                        *content = Value::String(format!(
                            "{}\n[...truncated {} chars...]",
                            kept,
                            s.len() - config.max_content_chars
                        ));
                    }
                }
            }
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

    let compressed_val = Value::Array(compressed);
    let original_tokens = estimate_tokens(messages) as i64;
    let compressed_tokens = estimate_tokens(&compressed_val) as i64;

    // Never-Worse 保护
    let final_val = never_worse(messages, &compressed_val);
    let final_tokens = estimate_tokens(&final_val) as i64;
    let saved = (original_tokens - final_tokens).max(0);

    (final_val, saved)
}
