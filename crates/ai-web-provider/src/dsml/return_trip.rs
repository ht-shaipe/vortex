//! 回程文本合成与网页注入：将 DSH 回传的 role=tool 消息转为自然语言文本，
//! 注入 deepseek.com 网页版输入框，让模型继续推理。
//!
//! 完整回程链路：
//! DSH 发回 role=tool → 查 InvokeRegistry 得工具名 → 合成自然语言文本
//! → WebInjector 注入网页 → 模型继续输出 → 解析新一轮 DSML

use serde_json::{json, Value};

use super::config::DsmlConfig;
use super::metrics::DsmlMetrics;
use super::registry::InvokeRegistry;

/// 合成回程文本：将 tool 消息列表转为自然语言格式。
///
/// `tool_messages`: `(call_id, content)` 列表
/// 返回 `(formatted_text, miss_count)`
pub fn format_tool_results(
    tool_messages: &[(String, String)],
    request_id: &str,
    registry: &InvokeRegistry,
    config: &DsmlConfig,
    metrics: &DsmlMetrics,
) -> (String, usize) {
    let mut parts = Vec::new();
    let mut miss_count = 0;

    for (call_id, content) in tool_messages {
        match registry.lookup(request_id, call_id) {
            Some(invoke) => {
                let part = match config.tool_result_format.as_str() {
                    "markdown" => {
                        format!("工具 `{}` 的执行结果：\n```\n{}\n```", invoke.name, content)
                    }
                    "json" => {
                        json!({
                            "tool": invoke.name,
                            "result": content,
                        })
                        .to_string()
                    }
                    _ => format!("工具 {} 的执行结果：\n{}", invoke.name, content),
                };
                parts.push(part);
            }
            None => {
                miss_count += 1;
                metrics.record_registry_miss();
                tracing::warn!(
                    request_id = request_id,
                    event = "registry_miss",
                    call_id = call_id,
                    "回程 registry 未命中，原样保留 content",
                );
                parts.push(content.clone());
            }
        }
    }

    (parts.join("\n\n"), miss_count)
}

/// 从 OpenAI 请求体提取 tool 消息列表：`(call_id, content)`。
pub fn extract_tool_messages(body: &Value) -> Vec<(String, String)> {
    let msgs = match body.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return vec![],
    };
    msgs.iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool"))
        .filter_map(|m| {
            let call_id = m
                .get("tool_call_id")
                .and_then(|v| v.as_str())?
                .to_string();
            let content = match m.get("content") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            Some((call_id, content))
        })
        .collect()
}

/// 判断请求是否含 role=tool 消息（用于 handler 分流）。
pub fn has_tool_messages(body: &Value) -> bool {
    body.get("messages")
        .and_then(|m| m.as_array())
        .map(|msgs| {
            msgs.iter()
                .any(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool"))
        })
        .unwrap_or(false)
}

/// 构造回程降级响应（注入失败/等待超时时使用）。
pub fn build_return_trip_degrade_response(
    error_msg: &str,
    model: &str,
    request_id: &str,
) -> Value {
    tracing::warn!(
        request_id = request_id,
        event = "return_trip_degrade",
        error = error_msg,
        "回程降级",
    );
    json!({
        "id": format!("chatcmpl-rt-degrade-{}", request_id),
        "object": "chat.completion",
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": format!("回程处理失败：{error_msg}"),
            },
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    })
}

/// 构造轮次上限响应。
pub fn build_max_rounds_response(model: &str, request_id: &str, max_rounds: u32) -> Value {
    json!({
        "id": format!("chatcmpl-rt-max-{}", request_id),
        "object": "chat.completion",
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": format!("已达最大回程轮次（{max_rounds}），强制结束。"),
            },
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsml::types::Invoke;
    use std::sync::Arc;

    fn mk_setup() -> (Arc<InvokeRegistry>, DsmlConfig, Arc<DsmlMetrics>) {
        let reg = Arc::new(InvokeRegistry::new(300));
        let cfg = DsmlConfig::default();
        let metrics = DsmlMetrics::register();
        reg.register(
            "req1",
            "call_abc",
            Invoke {
                name: "bash".into(),
                parameters: vec![],
            },
        );
        (reg, cfg, metrics)
    }

    #[test]
    fn test_format_hit() {
        let (reg, cfg, metrics) = mk_setup();
        let (text, miss) = format_tool_results(
            &[("call_abc".into(), "current dir: /tmp".into())],
            "req1",
            &reg,
            &cfg,
            &metrics,
        );
        assert_eq!(miss, 0);
        assert!(text.contains("工具 bash 的执行结果"));
        assert!(text.contains("current dir: /tmp"));
    }

    #[test]
    fn test_format_miss() {
        let (reg, cfg, metrics) = mk_setup();
        let (text, miss) = format_tool_results(
            &[("call_unknown".into(), "some result".into())],
            "req1",
            &reg,
            &cfg,
            &metrics,
        );
        assert_eq!(miss, 1);
        assert_eq!(text, "some result");
    }

    #[test]
    fn test_format_multiple() {
        let (reg, cfg, metrics) = mk_setup();
        let (text, miss) = format_tool_results(
            &[
                ("call_abc".into(), "result1".into()),
                ("call_abc".into(), "result2".into()),
            ],
            "req1",
            &reg,
            &cfg,
            &metrics,
        );
        assert_eq!(miss, 0);
        assert!(text.contains("\n\n"));
    }

    #[test]
    fn test_extract_tool_messages() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": null, "tool_calls": []},
                {"role": "tool", "tool_call_id": "call_1", "content": "result1"},
                {"role": "tool", "tool_call_id": "call_2", "content": "result2"},
            ]
        });
        let msgs = extract_tool_messages(&body);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], ("call_1".into(), "result1".into()));
        assert_eq!(msgs[1], ("call_2".into(), "result2".into()));
    }

    #[test]
    fn test_has_tool_messages() {
        let body_with = json!({
            "messages": [{"role": "tool", "tool_call_id": "x", "content": "y"}]
        });
        let body_without = json!({
            "messages": [{"role": "user", "content": "hi"}]
        });
        assert!(has_tool_messages(&body_with));
        assert!(!has_tool_messages(&body_without));
    }
}