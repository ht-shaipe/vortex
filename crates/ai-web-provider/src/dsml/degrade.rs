//! 降级与脱敏处理：为解析器与构造器提供统一的降级日志、脱敏、指标上报。

use serde_json::{json, Value};

use super::config::DsmlConfig;
use super::metrics::DsmlMetrics;
use super::types::DegradeReason;

/// 脱敏：依次应用 desensitize_rules 将匹配串替换为 `***`，输入先截取前 256 字符。
pub fn desensitize(snippet: &str, rules: &[regex::Regex]) -> String {
    let truncated: String = snippet.chars().take(256).collect();
    let mut result = truncated;
    for rule in rules {
        result = rule.replace_all(&result, "***").to_string();
    }
    result
}

/// 降级处理入口：脱敏 raw_snippet → 结构化日志 → Prometheus 指标。
pub fn handle(
    request_id: &str,
    reason: DegradeReason,
    raw_snippet: &str,
    config: &DsmlConfig,
    metrics: &DsmlMetrics,
) {
    let desensitized = desensitize(raw_snippet, &config.desensitize_rules);
    tracing::warn!(
        request_id = request_id,
        event = "degrade",
        reason = %reason,
        raw_snippet = %desensitized,
        "DSML 降级事件",
    );
    metrics.record_degrade(reason);
}

/// 构造全量降级响应（HTTP 200）：content 保留原文、无 tool_calls、finish_reason 透传。
pub fn build_degrade_response(original_content: &str, model: &str) -> Value {
    json!({
        "id": format!("chatcmpl-degrade-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)),
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
                "content": original_content,
            },
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    })
}

/// 合并部分降级：合法 tool_calls 已输出，非法项原文回填 content。
pub fn merge_partial_degrade(plain_text: &str, failed_snippets: &[String]) -> String {
    if failed_snippets.is_empty() {
        return plain_text.to_string();
    }
    let mut result = plain_text.to_string();
    for snippet in failed_snippets {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(snippet);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desensitize_base64() {
        let rules = vec![regex::Regex::new(r"[A-Za-z0-9+/]{16,}").unwrap()];
        let snippet = "token: AbCdEfGhIjKlMnOpQrSt";
        let result = desensitize(snippet, &rules);
        assert!(result.contains("***"));
    }

    #[test]
    fn test_desensitize_truncate() {
        let rules = vec![];
        let long = "x".repeat(300);
        let result = desensitize(&long, &rules);
        assert_eq!(result.chars().count(), 256);
    }

    #[test]
    fn test_merge_partial_degrade() {
        let result = merge_partial_degrade("ok", &["bad1".into(), "bad2".into()]);
        assert_eq!(result, "ok\nbad1\nbad2");
    }

    #[test]
    fn test_merge_no_failures() {
        let result = merge_partial_degrade("ok", &[]);
        assert_eq!(result, "ok");
    }
}