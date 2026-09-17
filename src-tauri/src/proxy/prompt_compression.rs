//! Prompt 压缩管线（opt-in）。
//!
//! 在路由前做去重、过滤 tool 输出、压缩重复 JSON、裁剪陈旧上下文。
//! fail-open 设计：压缩失败不阻塞请求。

use serde_json::Value;

/// 压缩配置
#[derive(Clone, Debug)]
pub struct CompressionConfig {
    /// 是否启用
    pub enabled: bool,
    /// 最大上下文消息数
    pub max_messages: usize,
    /// 是否去重连续相同角色的消息
    pub dedup_consecutive: bool,
    /// 是否裁剪 tool 输出（只保留最后 N 行）
    pub trim_tool_output: bool,
    /// tool 输出保留行数
    pub tool_output_lines: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_messages: 50,
            dedup_consecutive: true,
            trim_tool_output: true,
            tool_output_lines: 10,
        }
    }
}

/// 压缩消息列表
pub fn compress_messages(messages: &Value, config: &CompressionConfig) -> Value {
    if !config.enabled {
        return messages.clone();
    }

    let arr = match messages.as_array() {
        Some(a) => a,
        None => return messages.clone(),
    };

    let mut compressed: Vec<Value> = Vec::new();
    let mut last_role: Option<String> = None;

    for msg in arr {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();

        // 去重连续相同角色的消息
        if config.dedup_consecutive {
            if Some(&role) == last_role.as_ref() {
                if role == "system" {
                    continue;
                }
            }
        }
        last_role = Some(role.clone());

        // 裁剪 tool 输出
        let mut msg = msg.clone();
        if config.trim_tool_output && role == "tool" {
            if let Some(content) = msg.get_mut("content") {
                if let Some(s) = content.as_str() {
                    let lines: Vec<&str> = s.lines().collect();
                    if lines.len() > config.tool_output_lines {
                        let trimmed = lines[lines.len() - config.tool_output_lines..].join("\n");
                        *content = Value::String(format!("[...truncated {} lines...]\n{}", lines.len() - config.tool_output_lines, trimmed));
                    }
                }
            }
        }

        compressed.push(msg);
    }

    // 裁剪陈旧上下文：保留 system 消息 + 最后 N 条
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

    Value::Array(compressed)
}
