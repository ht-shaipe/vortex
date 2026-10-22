//! tool_calls 结构构造：将 invoke 列表映射为 OpenAI tool_calls JSON 数组。

use std::collections::HashSet;

use serde_json::{json, Value};

use super::config::DsmlConfig;
use super::types::{DegradeEvent, DegradeReason, Invoke};

/// 生成唯一 call_id：`call_` + 24 位小写 hex。
/// 同请求内冲突时重试，超限返回 Err(IdConflict)。
fn generate_call_id(
    id_set: &mut HashSet<String>,
    retry_limit: u8,
) -> Result<String, DegradeReason> {
    for _ in 0..retry_limit {
        let hex = uuid::Uuid::new_v4().simple().to_string();
        let id = format!("call_{}", &hex[..24]);
        if id_set.insert(id.clone()) {
            return Ok(id);
        }
    }
    Err(DegradeReason::IdConflict)
}

/// 校验工具名合法性：`^[a-zA-Z0-9_-]{1,64}$`
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 将 invoke 列表构造为 OpenAI tool_calls JSON 数组。
///
/// 返回 `(tool_calls, degrade_events)`：
/// - 工具名非法 → 记 `InvalidName` 并跳过
/// - id 冲突超限 → 计 `IdConflict` 并跳过
/// - arguments 序列化失败 → 计 `SerializeFail` 并跳过
/// - 顺序与 invokes 一致
pub fn build_tool_calls(
    invokes: &[Invoke],
    id_set: &mut HashSet<String>,
    config: &DsmlConfig,
) -> (Vec<Value>, Vec<DegradeEvent>) {
    let mut tool_calls = Vec::new();
    let mut degrade_events = Vec::new();

    for invoke in invokes {
        if !is_valid_name(&invoke.name) {
            degrade_events.push(DegradeEvent {
                reason: DegradeReason::InvalidName,
                raw_snippet: invoke.name.chars().take(256).collect(),
                invoke_index: Some(tool_calls.len()),
            });
            continue;
        }

        let id = match generate_call_id(id_set, config.id_retry_limit) {
            Ok(id) => id,
            Err(reason) => {
                degrade_events.push(DegradeEvent {
                    reason,
                    raw_snippet: invoke.name.chars().take(256).collect(),
                    invoke_index: Some(tool_calls.len()),
                });
                continue;
            }
        };

        let mut args_map = serde_json::Map::new();
        for param in &invoke.parameters {
            args_map.insert(param.name.clone(), json!(param.value));
        }
        let arguments = match serde_json::to_string(&Value::Object(args_map)) {
            Ok(s) => s,
            Err(_) => {
                degrade_events.push(DegradeEvent {
                    reason: DegradeReason::SerializeFail,
                    raw_snippet: invoke.name.chars().take(256).collect(),
                    invoke_index: Some(tool_calls.len()),
                });
                continue;
            }
        };

        tool_calls.push(json!({
            "id": id,
            "type": "function",
            "function": {
                "name": invoke.name,
                "arguments": arguments,
            }
        }));
    }

    (tool_calls, degrade_events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsml::types::Parameter;

    fn cfg() -> DsmlConfig {
        DsmlConfig::default()
    }

    #[test]
    fn test_id_format_and_uniqueness() {
        let invokes = vec![
            Invoke {
                name: "a".into(),
                parameters: vec![],
            },
            Invoke {
                name: "b".into(),
                parameters: vec![],
            },
            Invoke {
                name: "c".into(),
                parameters: vec![],
            },
        ];
        let mut id_set = HashSet::new();
        let (tc, deg) = build_tool_calls(&invokes, &mut id_set, &cfg());
        assert_eq!(tc.len(), 3);
        assert!(deg.is_empty());
        for call in &tc {
            let id = call["id"].as_str().unwrap();
            assert!(id.starts_with("call_"));
            assert_eq!(id.len(), 29); // "call_" (5) + 24 hex
        }
        let ids: Vec<&str> = tc.iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
    }

    #[test]
    fn test_arguments_serialization() {
        let invokes = vec![Invoke {
            name: "bash".into(),
            parameters: vec![
                Parameter {
                    name: "command".into(),
                    value: r#"pwd; echo "---""#.into(),
                    is_string: true,
                },
                Parameter {
                    name: "description".into(),
                    value: "show dir".into(),
                    is_string: true,
                },
            ],
        }];
        let mut id_set = HashSet::new();
        let (tc, _) = build_tool_calls(&invokes, &mut id_set, &cfg());
        assert_eq!(tc[0]["function"]["name"], "bash");
        let args = tc[0]["function"]["arguments"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(args).unwrap();
        assert_eq!(parsed["command"], r#"pwd; echo "---""#);
        assert_eq!(parsed["description"], "show dir");
    }

    #[test]
    fn test_empty_invoke_arguments() {
        let invokes = vec![Invoke {
            name: "noop".into(),
            parameters: vec![],
        }];
        let mut id_set = HashSet::new();
        let (tc, _) = build_tool_calls(&invokes, &mut id_set, &cfg());
        assert_eq!(tc[0]["function"]["arguments"], "{}");
    }

    #[test]
    fn test_invalid_name_rejected() {
        let invokes = vec![Invoke {
            name: "bad@name".into(),
            parameters: vec![],
        }];
        let mut id_set = HashSet::new();
        let (tc, deg) = build_tool_calls(&invokes, &mut id_set, &cfg());
        assert_eq!(tc.len(), 0);
        assert!(deg.iter().any(|e| e.reason == DegradeReason::InvalidName));
    }

    #[test]
    fn test_order_preserved() {
        let invokes = vec![
            Invoke {
                name: "bash".into(),
                parameters: vec![],
            },
            Invoke {
                name: "read".into(),
                parameters: vec![],
            },
            Invoke {
                name: "write".into(),
                parameters: vec![],
            },
        ];
        let mut id_set = HashSet::new();
        let (tc, _) = build_tool_calls(&invokes, &mut id_set, &cfg());
        assert_eq!(tc[0]["function"]["name"], "bash");
        assert_eq!(tc[1]["function"]["name"], "read");
        assert_eq!(tc[2]["function"]["name"], "write");
    }
}