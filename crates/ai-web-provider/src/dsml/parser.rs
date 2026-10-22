//! DSML 文本解析：手写状态机（标记搜索方式），精确处理全角竖线 `｜｜`（U+FF5C）。
//!
//! 状态流转：PlainText → InCalls → InInvoke → InParameter → Degrade
//! 采用标记搜索而非逐字符扫描，O(n) 复杂度，parameter 值可含任意字符。

use super::config::DsmlConfig;
use super::types::{DegradeEvent, DegradeReason, Invoke, Parameter, ParseResult};


const CALLS_OPEN: &str = "<\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} calls>";
const CALLS_CLOSE: &str = "</\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} calls>";
const INVOKE_OPEN_PREFIX: &str = "<\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} invoke ";
const INVOKE_CLOSE: &str = "</\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} invoke>";
const PARAM_OPEN_PREFIX: &str = "<\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} parameter ";
const PARAM_CLOSE: &str = "</\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} parameter>";

/// 提取标签属性值：从 `name="X"` 中提取 X，处理转义双引号。
fn extract_attr(attrs: &str, attr_name: &str) -> Option<String> {
    let prefix = format!("{attr_name}=\"");
    let start = attrs.find(&prefix)?;
    let val_start = start + prefix.len();
    let rest = &attrs[val_start..];
    let mut result = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                if next == '"' {
                    result.push('"');
                } else {
                    result.push('\\');
                    result.push(next);
                }
            }
        } else if c == '"' {
            return Some(result);
        } else {
            result.push(c);
        }
    }
    None
}

/// 解析 content 中的 DSML 调用块，返回 invoke 列表 + 剥离后的普通文本 + 降级事件。
///
/// `tools_whitelist` 为 `None` 时跳过解析，原样返回 content 为 `plain_text`。
pub fn parse_content(
    content: &str,
    tools_whitelist: Option<&[&str]>,
    _config: &DsmlConfig,
) -> ParseResult {
    if tools_whitelist.is_none() {
        return ParseResult {
            invokes: vec![],
            plain_text: content.to_string(),
            degrade_events: vec![],
        };
    }

    let mut invokes: Vec<Invoke> = Vec::new();
    let mut plain_text = String::new();
    let mut degrade_events: Vec<DegradeEvent> = Vec::new();

    let mut pos = 0;
    while pos < content.len() {
        match content[pos..].find(CALLS_OPEN) {
            Some(offset) => {
                plain_text.push_str(&content[pos..pos + offset]);
                let block_start = pos + offset;
                let body_start = block_start + CALLS_OPEN.len();

                match content[body_start..].find(CALLS_CLOSE) {
                    Some(close_off) => {
                        let block_body = &content[body_start..body_start + close_off];
                        parse_invokes(
                            block_body,
                            &mut invokes,
                            &mut degrade_events,
                        );
                        pos = body_start + close_off + CALLS_CLOSE.len();
                    }
                    None => {
                        degrade_events.push(DegradeEvent {
                            reason: DegradeReason::UnclosedBlock,
                            raw_snippet: content[block_start..].chars().take(256).collect(),
                            invoke_index: None,
                        });
                        plain_text.push_str(&content[block_start..]);
                        pos = content.len();
                    }
                }
            }
            None => {
                plain_text.push_str(&content[pos..]);
                pos = content.len();
            }
        }
    }

    ParseResult {
        invokes,
        plain_text,
        degrade_events,
    }
}

/// 解析 calls 块内的 invoke 列表。
fn parse_invokes(
    block: &str,
    invokes: &mut Vec<Invoke>,
    degrade_events: &mut Vec<DegradeEvent>,
) {
    let mut pos = 0;
    while pos < block.len() {
        match block[pos..].find(INVOKE_OPEN_PREFIX) {
            Some(offset) => {
                let invoke_start = pos + offset;
                let attr_start = invoke_start + INVOKE_OPEN_PREFIX.len();

                match block[attr_start..].find('>') {
                    Some(gt_off) => {
                        let attrs = &block[attr_start..attr_start + gt_off];
                        let name = extract_attr(attrs, "name").unwrap_or_default();
                        let after_tag = attr_start + gt_off + 1;

                        match block[after_tag..].find(INVOKE_CLOSE) {
                            Some(close_off) => {
                                let invoke_body = &block[after_tag..after_tag + close_off];
                                let parameters =
                                    parse_parameters(invoke_body, degrade_events);
                                invokes.push(Invoke { name, parameters });
                                pos = after_tag + close_off + INVOKE_CLOSE.len();
                            }
                            None => {
                                degrade_events.push(DegradeEvent {
                                    reason: DegradeReason::UnclosedBlock,
                                    raw_snippet: block[invoke_start..]
                                        .chars()
                                        .take(256)
                                        .collect(),
                                    invoke_index: Some(invokes.len()),
                                });
                                pos = block.len();
                            }
                        }
                    }
                    None => {
                        pos = block.len();
                    }
                }
            }
            None => {
                pos = block.len();
            }
        }
    }
}

/// 解析 invoke body 内的 parameter 列表。同名 parameter 后值覆盖前值 + 告警。
fn parse_parameters(
    body: &str,
    degrade_events: &mut Vec<DegradeEvent>,
) -> Vec<Parameter> {
    let mut params: Vec<Parameter> = Vec::new();
    let mut pos = 0;
    while pos < body.len() {
        match body[pos..].find(PARAM_OPEN_PREFIX) {
            Some(offset) => {
                let param_start = pos + offset;
                let attr_start = param_start + PARAM_OPEN_PREFIX.len();

                match body[attr_start..].find('>') {
                    Some(gt_off) => {
                        let attrs = &body[attr_start..attr_start + gt_off];
                        let name = extract_attr(attrs, "name").unwrap_or_default();
                        let is_string = extract_attr(attrs, "string")
                            .map(|v| v == "true")
                            .unwrap_or(true);
                        let after_tag = attr_start + gt_off + 1;

                        match body[after_tag..].find(PARAM_CLOSE) {
                            Some(close_off) => {
                                let value =
                                    body[after_tag..after_tag + close_off].to_string();

                                if let Some(existing) =
                                    params.iter_mut().find(|p| p.name == name)
                                {
                                    existing.value = value;
                                    degrade_events.push(DegradeEvent {
                                        reason: DegradeReason::DuplicateParam,
                                        raw_snippet: name.chars().take(256).collect(),
                                        invoke_index: None,
                                    });
                                } else {
                                    params.push(Parameter {
                                        name,
                                        value,
                                        is_string,
                                    });
                                }
                                pos = after_tag + close_off + PARAM_CLOSE.len();
                            }
                            None => {
                                degrade_events.push(DegradeEvent {
                                    reason: DegradeReason::UnclosedBlock,
                                    raw_snippet: body[param_start..]
                                        .chars()
                                        .take(256)
                                        .collect(),
                                    invoke_index: None,
                                });
                                pos = body.len();
                            }
                        }
                    }
                    None => {
                        pos = body.len();
                    }
                }
            }
            None => {
                pos = body.len();
            }
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    const DSML_BAR: char = '\u{FF5C}';

    fn make_dsml_invoke(name: &str, params: &[(&str, &str)]) -> String {
        let bar = format!("{DSML_BAR}{DSML_BAR}");
        let mut s = format!("<{bar}DSML{bar} invoke name=\"{name}\">");
        for (pname, pval) in params {
            s.push_str(&format!(
                "<{bar}DSML{bar} parameter name=\"{pname}\" string=\"true\">{pval}</{bar}DSML{bar} parameter>"
            ));
        }
        s.push_str(&format!("</{bar}DSML{bar} invoke>"));
        s
    }

    fn make_dsml_block(invokes: &[&str]) -> String {
        let bar = format!("{DSML_BAR}{DSML_BAR}");
        let mut s = format!("<{bar}DSML{bar} calls>");
        for inv in invokes {
            s.push_str(inv);
        }
        s.push_str(&format!("</{bar}DSML{bar} calls>"));
        s
    }

    fn default_config() -> DsmlConfig {
        DsmlConfig::default()
    }

    #[test]
    fn test_parse_single_invoke() {
        let inv = make_dsml_invoke(
            "bash",
            &[("command", "pwd"), ("description", "show dir")],
        );
        let block = make_dsml_block(&[&inv]);
        let content = format!("好的，我来执行：\n{block}");
        let result = parse_content(&content, Some(&["bash"]), &default_config());
        assert_eq!(result.invokes.len(), 1);
        assert_eq!(result.invokes[0].name, "bash");
        assert_eq!(result.invokes[0].parameters.len(), 2);
        assert_eq!(result.invokes[0].parameters[0].name, "command");
        assert_eq!(result.invokes[0].parameters[0].value, "pwd");
        assert_eq!(result.invokes[0].parameters[1].name, "description");
        assert_eq!(result.invokes[0].parameters[1].value, "show dir");
        assert_eq!(result.plain_text, "好的，我来执行：\n");
    }

    #[test]
    fn test_half_width_bar_not_recognized() {
        let content = r#"<||DSML|| calls><||DSML|| invoke name="bash"></||DSML|| invoke></||DSML|| calls>"#;
        let result = parse_content(content, Some(&["bash"]), &default_config());
        assert_eq!(result.invokes.len(), 0);
        assert_eq!(result.plain_text, content);
    }

    #[test]
    fn test_no_whitelist_passthrough() {
        let inv = make_dsml_invoke("bash", &[("command", "pwd")]);
        let block = make_dsml_block(&[&inv]);
        let result = parse_content(&block, None, &default_config());
        assert_eq!(result.invokes.len(), 0);
        assert_eq!(result.plain_text, block);
    }

    #[test]
    fn test_unclosed_block_degrade() {
        let bar = format!("{DSML_BAR}{DSML_BAR}");
        let content = format!("<{bar}DSML{bar} calls><{bar}DSML{bar} invoke name=\"bash\">");
        let result = parse_content(&content, Some(&["bash"]), &default_config());
        assert_eq!(result.invokes.len(), 0);
        assert!(result
            .degrade_events
            .iter()
            .any(|e| e.reason == DegradeReason::UnclosedBlock));
    }

    #[test]
    fn test_multiple_invokes_order() {
        let inv1 = make_dsml_invoke("bash", &[("command", "ls")]);
        let inv2 = make_dsml_invoke("read", &[("path", "/tmp")]);
        let inv3 = make_dsml_invoke("write", &[("path", "/etc")]);
        let block = make_dsml_block(&[&inv1, &inv2, &inv3]);
        let result = parse_content(&block, Some(&["bash", "read", "write"]), &default_config());
        assert_eq!(result.invokes.len(), 3);
        assert_eq!(result.invokes[0].name, "bash");
        assert_eq!(result.invokes[1].name, "read");
        assert_eq!(result.invokes[2].name, "write");
    }

    #[test]
    fn test_multiple_blocks_merge() {
        let inv1 = make_dsml_invoke("bash", &[("command", "ls")]);
        let inv2 = make_dsml_invoke("read", &[("path", "/tmp")]);
        let block1 = make_dsml_block(&[&inv1]);
        let block2 = make_dsml_block(&[&inv2]);
        let content = format!("prefix\n{block1}middle\n{block2}suffix");
        let result =
            parse_content(&content, Some(&["bash", "read"]), &default_config());
        assert_eq!(result.invokes.len(), 2);
        assert_eq!(result.invokes[0].name, "bash");
        assert_eq!(result.invokes[1].name, "read");
        assert_eq!(result.plain_text, "prefix\nmiddle\nsuffix");
    }

    #[test]
    fn test_param_value_with_special_chars() {
        let inv = make_dsml_invoke("bash", &[("command", "echo '<div>' & \n test")]);
        let block = make_dsml_block(&[&inv]);
        let result = parse_content(&block, Some(&["bash"]), &default_config());
        assert_eq!(result.invokes[0].parameters[0].value, "echo '<div>' & \n test");
    }

    #[test]
    fn test_duplicate_param_overwrite() {
        let bar = format!("{DSML_BAR}{DSML_BAR}");
        let inv = format!(
            "<{bar}DSML{bar} invoke name=\"bash\">\
             <{bar}DSML{bar} parameter name=\"x\" string=\"true\">1</{bar}DSML{bar} parameter>\
             <{bar}DSML{bar} parameter name=\"x\" string=\"true\">2</{bar}DSML{bar} parameter>\
             </{bar}DSML{bar} invoke>"
        );
        let block = make_dsml_block(&[&inv]);
        let result = parse_content(&block, Some(&["bash"]), &default_config());
        assert_eq!(result.invokes[0].parameters.len(), 1);
        assert_eq!(result.invokes[0].parameters[0].value, "2");
        assert!(result
            .degrade_events
            .iter()
            .any(|e| e.reason == DegradeReason::DuplicateParam));
    }

    #[test]
    fn test_empty_invoke() {
        let bar = format!("{DSML_BAR}{DSML_BAR}");
        let inv = format!("<{bar}DSML{bar} invoke name=\"noop\"></{bar}DSML{bar} invoke>");
        let block = make_dsml_block(&[&inv]);
        let result = parse_content(&block, Some(&["noop"]), &default_config());
        assert_eq!(result.invokes.len(), 1);
        assert_eq!(result.invokes[0].parameters.len(), 0);
    }

    #[test]
    fn test_non_dsml_tag_preserved() {
        let content = "hello <tool>bash</tool> world";
        let result = parse_content(content, Some(&["bash"]), &default_config());
        assert_eq!(result.invokes.len(), 0);
        assert_eq!(result.plain_text, content);
    }
}