//! 响应/请求格式转换器
//!
//! 统一在 OpenAI 格式与 Anthropic / Gemini 格式之间转换。
//! Function Calling 全链路（tools / tool_choice / tool_calls / tool_call_id）
//! 在转换中保持完整，缺失字段会被严格校验的上游拒绝。

use serde_json::{json, Value};

// =============================================================================
// 文本提取
// =============================================================================

/// 提取消息 content 中的纯文本。
/// 兼容字符串与内容块数组两种形式（如 OpenAI 的 content parts、
/// Anthropic 的 content blocks）。
pub fn extract_text_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

// =============================================================================
// Anthropic → OpenAI（上游为 Anthropic 时）
// =============================================================================

/// 将 Anthropic Messages 响应转换为 OpenAI chat.completion 格式。
/// 支持 text 与 tool_use 内容块。
pub fn anthropic_to_openai_response(body: &Value, model: &str) -> Value {
    let blocks = body.get("content").and_then(|c| c.as_array());

    let text = blocks
        .map(|arr| {
            arr.iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    // tool_use 块 → OpenAI tool_calls
    let tool_calls: Vec<Value> = blocks
        .map(|arr| {
            arr.iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                .map(|b| {
                    json!({
                        "id": b.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        "type": "function",
                        "function": {
                            "name": b.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                            "arguments": b.get("input").map(|i| i.to_string()).unwrap_or_else(|| "{}".to_string())
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let input_tokens = body
        .get("usage")
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output_tokens = body
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // 有 tool_use 时 finish_reason 必须是 "tool_calls"
    let finish_reason = if !tool_calls.is_empty() {
        "tool_calls".to_string()
    } else {
        match body.get("stop_reason").and_then(|v| v.as_str()) {
            Some("end_turn") | None => "stop".to_string(),
            Some("tool_use") => "tool_calls".to_string(),
            Some("max_tokens") => "length".to_string(),
            Some(other) => other.to_string(),
        }
    };

    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("chatcmpl-anthropic")
        .to_string();

    // assistant 携带 tool_calls 且无文本时，content 必须为 null 而非 ""
    let content_value = if tool_calls.is_empty() {
        json!(text)
    } else if text.is_empty() {
        Value::Null
    } else {
        json!(text)
    };

    let mut message = json!({"role": "assistant", "content": content_value});
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    json!({
        "id": id,
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }
    })
}

// =============================================================================
// Gemini → OpenAI（上游为 Gemini 时）
// =============================================================================

pub fn gemini_to_openai_response(body: &Value, model: &str) -> Value {
    let content = body
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|cand| cand.get("content"))
        .and_then(|cont| cont.get("parts"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    let finish_reason = body
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|cand| cand.get("finishReason"))
        .and_then(|f| f.as_str())
        .map(|r| if r == "STOP" { "stop" } else { r })
        .unwrap_or("stop");

    let input_tokens = body
        .get("usageMetadata")
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output_tokens = body
        .get("usageMetadata")
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    json!({
        "id": format!("chatcmpl-gemini-{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": finish_reason
        }],
        "usage": {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }
    })
}

// =============================================================================
// OpenAI → Anthropic（入站为 Anthropic 协议、上游执行后回转换）
// =============================================================================

/// 将 OpenAI chat.completion 响应转换为 Anthropic Messages 响应格式。
pub fn openai_to_anthropic_response(body: &Value, original_model: &str) -> Value {
    let choice = body.get("choices").and_then(|c| c.get(0)).cloned().unwrap_or(json!({}));
    let message = choice.get("message").cloned().unwrap_or(json!({}));

    let text = message.get("content").and_then(|c| c.as_str()).unwrap_or("");

    let mut content_blocks = vec![];
    if !text.is_empty() {
        content_blocks.push(json!({"type": "text", "text": text}));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tool_calls {
            let func = tc.get("function").cloned().unwrap_or(json!({}));
            let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            content_blocks.push(json!({
                "type": "tool_use",
                "id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "name": func.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "input": input
            }));
        }
    }

    let has_tool_use = content_blocks
        .iter()
        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"));

    let stop_reason = if has_tool_use {
        "tool_use".to_string()
    } else {
        match choice.get("finish_reason").and_then(|f| f.as_str()) {
            Some("stop") | None => "end_turn".to_string(),
            Some("tool_calls") => "tool_use".to_string(),
            Some("length") => "max_tokens".to_string(),
            Some(other) => other.to_string(),
        }
    };

    let prompt_tokens = body
        .get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let completion_tokens = body
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4().to_string().replace("-", "")),
        "type": "message",
        "role": "assistant",
        "content": content_blocks,
        "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(original_model),
        "stop_reason": stop_reason,
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": prompt_tokens,
            "output_tokens": completion_tokens
        }
    })
}

// =============================================================================
// 请求方向的工具/消息转换（OpenAI 请求 → Anthropic 上游）
// =============================================================================

/// OpenAI tools 定义 → Anthropic tools 定义
pub fn openai_tools_to_anthropic(tools: &Value) -> Option<Value> {
    let arr = tools.as_array()?;
    let converted: Vec<Value> = arr
        .iter()
        .filter_map(|t| {
            let func = if t.get("type").and_then(|v| v.as_str()) == Some("function") {
                t.get("function")?
            } else {
                t
            };
            Some(json!({
                "name": func.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "description": func.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                "input_schema": func.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}}))
            }))
        })
        .collect();
    if converted.is_empty() {
        None
    } else {
        Some(json!(converted))
    }
}

/// OpenAI tool_choice → Anthropic tool_choice
pub fn openai_tool_choice_to_anthropic(tool_choice: &Value) -> Option<Value> {
    match tool_choice {
        Value::String(s) => match s.as_str() {
            "auto" => Some(json!({"type": "auto"})),
            "required" => Some(json!({"type": "any"})),
            "none" => None,
            _ => Some(json!({"type": "auto"})),
        },
        Value::Object(_) => {
            let name = tool_choice
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())?;
            Some(json!({"type": "tool", "name": name}))
        }
        _ => None,
    }
}

/// 将 OpenAI 消息数组转换为 Anthropic 消息数组。
/// 返回 (system, messages)。
///
/// 转换要点：
/// - system 消息 → 顶层 `system` 参数
/// - assistant 的 `tool_calls` → `tool_use` 内容块
/// - tool 角色消息（`tool_call_id`）→ user 的 `tool_result` 内容块
/// - 连续同角色消息合并（Anthropic 要求 user/assistant 交替）
pub fn openai_messages_to_anthropic(body: &Value) -> (Option<Value>, Vec<Value>) {
    let mut system: Option<Value> = None;
    let mut out: Vec<Value> = Vec::new();

    let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) else {
        return (system, out);
    };

    for msg in msgs {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        match role {
            "system" => {
                if system.is_none() {
                    system = Some(Value::String(extract_text_content(msg.get("content"))));
                }
            }
            "tool" => {
                let tool_use_id = msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("");
                let content = extract_text_content(msg.get("content"));
                push_merge(
                    &mut out,
                    "user",
                    json!([{"type": "tool_result", "tool_use_id": tool_use_id, "content": content}]),
                );
            }
            "assistant" => {
                let mut blocks = vec![];
                let text = extract_text_content(msg.get("content"));
                if !text.is_empty() {
                    blocks.push(json!({"type": "text", "text": text}));
                }
                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let func = tc.get("function").cloned().unwrap_or(json!({}));
                        let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                            "name": func.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                            "input": input
                        }));
                    }
                }
                if blocks.is_empty() {
                    blocks.push(json!({"type": "text", "text": ""}));
                }
                push_merge(&mut out, "assistant", json!(blocks));
            }
            _ => {
                let content = match msg.get("content") {
                    Some(Value::String(s)) => Value::String(s.clone()),
                    Some(Value::Array(parts)) => Value::Array(
                        parts
                            .iter()
                            .filter_map(|p| {
                                if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    p.get("text")
                                        .and_then(|t| t.as_str())
                                        .map(|t| json!({"type": "text", "text": t}))
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    ),
                    _ => Value::String(String::new()),
                };
                push_merge(&mut out, "user", content);
            }
        }
    }

    (system, out)
}

/// 追加消息，连续同角色时合并 content
fn push_merge(out: &mut Vec<Value>, role: &str, content: Value) {
    if let Some(last) = out.last_mut() {
        if last.get("role").and_then(|r| r.as_str()) == Some(role) {
            let merged = merge_content(last.get("content"), &content);
            last["content"] = merged;
            return;
        }
    }
    out.push(json!({"role": role, "content": content}));
}

fn merge_content(a: Option<&Value>, b: &Value) -> Value {
    let to_blocks = |v: &Value| -> Vec<Value> {
        match v {
            Value::String(s) => vec![json!({"type": "text", "text": s})],
            Value::Array(arr) => arr.clone(),
            _ => vec![],
        }
    };
    let a_blocks = a.map(to_blocks).unwrap_or_default();
    let b_blocks = to_blocks(b);
    let mut merged = a_blocks;
    merged.extend(b_blocks);
    json!(merged)
}

// =============================================================================
// 请求方向的工具/消息转换（Anthropic 入站请求 → OpenAI 内部格式）
// =============================================================================

/// 将 Anthropic 入站请求体转换为 OpenAI 格式请求体（model 除外，由调用方填充）。
pub fn anthropic_request_to_openai_body(body: &Value) -> Value {
    let mut messages: Vec<Value> = Vec::new();

    if let Some(sys) = body.get("system") {
        let text = extract_text_content(Some(sys));
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }

    if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = msg.get("content");

            match role {
                "assistant" => {
                    let (text, tool_calls) = anthropic_blocks_to_openai(content);
                    if tool_calls.is_empty() {
                        messages.push(json!({"role": "assistant", "content": text}));
                    } else {
                        // assistant 携带 tool_calls 时 content 必须为 null 而非 ""
                        let content_value = if text.is_empty() { Value::Null } else { json!(text) };
                        messages.push(json!({
                            "role": "assistant",
                            "content": content_value,
                            "tool_calls": tool_calls
                        }));
                    }
                }
                _ => {
                    // user：text 块 + tool_result 块（tool_result 转为 tool 角色消息）
                    if let Some(Value::Array(blocks)) = content {
                        let mut text_parts = Vec::new();
                        for block in blocks {
                            match block.get("type").and_then(|t| t.as_str()) {
                                Some("text") => {
                                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                        text_parts.push(t.to_string());
                                    }
                                }
                                Some("tool_result") => {
                                    let tool_use_id = block
                                        .get("tool_use_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let text = extract_text_content(block.get("content"));
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tool_use_id,
                                        "content": text
                                    }));
                                }
                                _ => {}
                            }
                        }
                        let text = text_parts.join("");
                        if !text.is_empty() || messages.is_empty() {
                            messages.push(json!({"role": "user", "content": text}));
                        }
                    } else {
                        let text = extract_text_content(content);
                        messages.push(json!({"role": "user", "content": text}));
                    }
                }
            }
        }
    }

    let mut openai_body = json!({"messages": messages});

    if let Some(mt) = body.get("max_tokens").and_then(|v| v.as_i64()) {
        openai_body["max_tokens"] = json!(mt);
    }
    if let Some(t) = body.get("temperature").and_then(|v| v.as_f64()) {
        openai_body["temperature"] = json!(t);
    }
    if let Some(t) = body.get("top_p").and_then(|v| v.as_f64()) {
        openai_body["top_p"] = json!(t);
    }
    if let Some(ss) = body.get("stop_sequences").and_then(|v| v.as_array()) {
        openai_body["stop"] = json!(ss);
    }

    // Anthropic tools → OpenAI tools
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let converted: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "description": t.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                        "parameters": t.get("input_schema").cloned().unwrap_or(json!({"type": "object", "properties": {}}))
                    }
                })
            })
            .collect();
        if !converted.is_empty() {
            openai_body["tools"] = json!(converted);

            if let Some(tc) = body.get("tool_choice") {
                let converted_tc = match tc.get("type").and_then(|t| t.as_str()) {
                    Some("auto") => json!("auto"),
                    Some("any") => json!("required"),
                    Some("tool") => json!({
                        "type": "function",
                        "function": {"name": tc.get("name").and_then(|n| n.as_str()).unwrap_or("")}
                    }),
                    _ => json!("auto"),
                };
                openai_body["tool_choice"] = converted_tc;
            }
        }
    }

    openai_body
}

/// Anthropic 内容块 → (纯文本, OpenAI tool_calls)
fn anthropic_blocks_to_openai(content: Option<&Value>) -> (String, Vec<Value>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    if let Some(Value::Array(blocks)) = content {
        for block in blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(json!({
                        "id": block.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        "type": "function",
                        "function": {
                            "name": block.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                            "arguments": block.get("input").map(|i| i.to_string()).unwrap_or_else(|| "{}".to_string())
                        }
                    }));
                }
                _ => {}
            }
        }
    } else if let Some(t) = content.and_then(|c| c.as_str()) {
        text.push_str(t);
    }
    (text, tool_calls)
}
