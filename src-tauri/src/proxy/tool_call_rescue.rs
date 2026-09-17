//! Tool-call rescue：将纯文本 tool call 救援为结构化 tool_calls。
//!
//! 有些模型把 tool call 以纯文本形式输出而非结构化 JSON，
//! 本模块检测并转换这些"伪 tool call"为真正的 OpenAI tool_calls 格式。

use serde_json::{json, Value};

/// 检测并救援纯文本 tool call
pub fn rescue_tool_calls(content: &str) -> Option<Value> {
    let markers = ["[TOOL_CALL]", "```tool_call", "```json"];
    for marker in &markers {
        if let Some(start) = content.find(marker) {
            let rest = &content[start + marker.len()..];
            let inner = if marker.starts_with("```") {
                let end = rest.find("```").unwrap_or(rest.len());
                rest[..end].trim()
            } else {
                rest.trim()
            };
            if let Ok(parsed) = serde_json::from_str::<Value>(inner) {
                return convert_to_tool_calls(&parsed);
            }
        }
    }

    let trimmed = content.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            if parsed.get("name").is_some() && (parsed.get("arguments").is_some() || parsed.get("parameters").is_some()) {
                return convert_to_tool_calls(&parsed);
            }
        }
    }

    None
}

fn convert_to_tool_calls(parsed: &Value) -> Option<Value> {
    let name = parsed.get("name")?.as_str()?;
    let arguments = parsed
        .get("arguments")
        .or_else(|| parsed.get("parameters"))
        .cloned()
        .unwrap_or(json!({}));

    let tool_call = json!({
        "id": format!("call_{}", uuid::Uuid::new_v4().simple()),
        "type": "function",
        "function": {
            "name": name,
            "arguments": serde_json::to_string(&arguments).unwrap_or_default(),
        }
    });

    Some(json!([tool_call]))
}

pub fn rescue_from_response(body: &mut Value) -> bool {
    let choices = match body.get_mut("choices") {
        Some(c) => c,
        None => return false,
    };
    let choices_arr = match choices.as_array_mut() {
        Some(a) => a,
        None => return false,
    };
    let mut rescued = false;

    for choice in choices_arr {
        let message = match choice.get_mut("message") {
            Some(m) => m,
            None => continue,
        };
        let content = match message.get("content") {
            Some(c) => c.as_str().map(|s| s.to_string()),
            None => continue,
        };
        let content = match content {
            Some(c) => c,
            None => continue,
        };

        if let Some(tool_calls) = rescue_tool_calls(&content) {
            if let Some(existing) = message.get_mut("tool_calls") {
                if let Some(arr) = existing.as_array_mut() {
                    if let Some(new_arr) = tool_calls.as_array() {
                        arr.extend(new_arr.iter().cloned());
                    }
                }
            } else {
                message["tool_calls"] = tool_calls;
            }
            message["content"] = json!(null);
            rescued = true;
        }
    }

    rescued
}
