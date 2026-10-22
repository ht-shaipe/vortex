//! DSML（DeepSeek Markup Language）解析与 tool_calls 转换模块。
//!
//! 将 deepseek.com 网页版返回的 DSML 格式文本（`<｜｜DSML｜｜ calls>` 调用块）
//! 解析为 OpenAI 标准的 `tool_calls` 结构，使 DSH 等 OpenAI 协议客户端
//! 能正确识别结构化工具调用，而非 `content` 字段里的普通文本。
//!
//! 双向链路：
//! - 正向：DSML 文本 → [`parser::parse_content`] → [`builder::build_tool_calls`]
//!   → OpenAI `tool_calls` + `finish_reason: "tool_calls"`
//! - 回程：DSH 发回 `role=tool` 消息 → [`return_trip::format_tool_results`]
//!   → 自然语言文本 → 注入网页输入框 → 模型继续输出

pub mod builder;
pub mod config;
pub mod degrade;
pub mod metrics;
pub mod parser;
pub mod registry;
pub mod return_trip;
pub mod stream;
pub mod types;

pub use builder::build_tool_calls;
pub use config::DsmlConfig;
pub use metrics::DsmlMetrics;
pub use parser::parse_content;
pub use types::*;

use std::collections::HashSet;

use serde_json::{json, Value};

/// 将 DSML 文本转换为 OpenAI chat.completion 响应。
///
/// 正向链路核心：parse_content → build_tool_calls → 构造响应。
/// - 含 tool_calls 时：`role=assistant`、`finish_reason=tool_calls`、content 为 plain_text
/// - 无 tool_calls 时：`role=assistant`、`finish_reason=stop`、content 透传
/// - 双写模式：content 保留原文 + tool_calls 并存
/// - content 为空且 tool_calls 非空时 content 设为 null
pub fn dsml_to_openai_completion(
    content: &str,
    thinking: &str,
    tools_whitelist: Option<&[&str]>,
    model: &str,
    config: &DsmlConfig,
    metrics: &DsmlMetrics,
    request_id: &str,
    chatcmpl_id: &str,
    created: u64,
) -> Value {
    let start = std::time::Instant::now();

    let parse_result = parse_content(content, tools_whitelist, config);
    let mut id_set = HashSet::new();
    let (tool_calls, builder_degrades) = build_tool_calls(&parse_result.invokes, &mut id_set, config);

    for de in &parse_result.degrade_events {
        degrade::handle(request_id, de.reason, &de.raw_snippet, config, metrics);
    }
    for de in &builder_degrades {
        degrade::handle(request_id, de.reason, &de.raw_snippet, config, metrics);
    }

    let has_tool_calls = !tool_calls.is_empty();
    let latency_ms = start.elapsed().as_millis() as u64;
    if has_tool_calls {
        metrics.record_parse_ok(latency_ms);
    }

    let message_content: Value = if has_tool_calls {
        if config.dual_write_mode {
            json!(content)
        } else if parse_result.plain_text.is_empty() {
            Value::Null
        } else {
            json!(parse_result.plain_text)
        }
    } else {
        json!(content)
    };

    let mut message = json!({
        "role": "assistant",
        "content": message_content,
    });
    if !thinking.is_empty() {
        message["reasoning_content"] = json!(thinking);
    }
    if has_tool_calls {
        message["tool_calls"] = Value::Array(tool_calls);
    }

    let finish_reason = if has_tool_calls { "tool_calls" } else { "stop" };

    json!({
        "id": format!("chatcmpl-{chatcmpl_id}"),
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason,
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    })
}

/// 从 OpenAI 请求体提取 tools 白名单（工具名列表）。
pub fn extract_tools_whitelist(body: &Value) -> Option<Vec<String>> {
    let tools = body.get("tools")?.as_array()?;
    let names: Vec<String> = tools
        .iter()
        .filter_map(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(String::from)
        })
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}