//! 流式增量解析状态机：跨 chunk 缓冲 DSML 片段，闭合后输出 tool_call delta。
//!
//! 双态：StreamingPlainText（实时透传）/ StreamingDsmlBuffer（跨 chunk 缓冲）

use std::collections::HashSet;

use super::config::DsmlConfig;
use super::metrics::DsmlMetrics;
use super::parser::parse_content;
use super::types::{DeltaEvent, DegradeReason};
use std::sync::Arc;

/// DSML 流式增量解析器。
pub struct DsmlStreamParser {
    dsml_buffer: String,
    plain_pending: String,
    current_index: usize,
    config: DsmlConfig,
    tools_whitelist: Option<Vec<String>>,
    id_set: HashSet<String>,
    metrics: Arc<DsmlMetrics>,
    request_id: String,
    in_dsml: bool,
}

const DSML_TRIGGER: &str = "<\u{FF5C}\u{FF5C}DSML";
const CALLS_CLOSE: &str = "</\u{FF5C}\u{FF5C}DSML\u{FF5C}\u{FF5C} calls>";

impl DsmlStreamParser {
    pub fn new(
        config: DsmlConfig,
        tools_whitelist: Option<Vec<String>>,
        metrics: Arc<DsmlMetrics>,
        request_id: String,
    ) -> Self {
        Self {
            dsml_buffer: String::new(),
            plain_pending: String::new(),
            current_index: 0,
            config,
            tools_whitelist,
            id_set: HashSet::new(),
            metrics,
            request_id,
            in_dsml: false,
        }
    }

    /// 增量处理一个 chunk，返回产出的 DeltaEvent 列表。
    pub fn feed(&mut self, chunk: &str) -> Vec<DeltaEvent> {
        let mut events = Vec::new();

        if self.tools_whitelist.is_none() {
            events.push(DeltaEvent::Content(chunk.to_string()));
            return events;
        }

        let combined = if !self.plain_pending.is_empty() {
            let mut s = std::mem::take(&mut self.plain_pending);
            s.push_str(chunk);
            s
        } else {
            chunk.to_string()
        };

        let mut remaining = &combined[..];
        while !remaining.is_empty() {
            if !self.in_dsml {
                match remaining.find(DSML_TRIGGER) {
                    Some(pos) => {
                        if pos > 0 {
                            events.push(DeltaEvent::Content(remaining[..pos].to_string()));
                        }
                        self.in_dsml = true;
                        self.dsml_buffer
                            .push_str(&remaining[pos..]);
                        remaining = "";
                    }
                    None => {
                        let trigger_bytes = DSML_TRIGGER.as_bytes();
                        let remaining_bytes = remaining.as_bytes();
                        let mut keep = 0;
                        for pl in 1..=remaining_bytes.len().min(trigger_bytes.len()) {
                            if &trigger_bytes[..pl] == &remaining_bytes[remaining_bytes.len() - pl..] {
                                keep = pl;
                            }
                        }
                        if keep > 0 && keep < remaining_bytes.len() {
                            let target = remaining_bytes.len() - keep;
                            let safe = remaining
                                .char_indices()
                                .take_while(|(i, _)| *i <= target)
                                .last()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            if safe > 0 {
                                events.push(DeltaEvent::Content(remaining[..safe].to_string()));
                            }
                            self.plain_pending = remaining[safe..].to_string();
                        } else if keep == remaining_bytes.len() {
                            self.plain_pending = remaining.to_string();
                        } else {
                            events.push(DeltaEvent::Content(remaining.to_string()));
                        }
                        remaining = "";
                    }
                }
            } else {
                self.dsml_buffer.push_str(remaining);
                remaining = "";

                if self.dsml_buffer.len() > self.config.buffer_limit {
                    events.push(DeltaEvent::Content(self.dsml_buffer.clone()));
                    events.push(DeltaEvent::Degrade {
                        reason: DegradeReason::BufferOverflow,
                        snippet: self.dsml_buffer.chars().take(256).collect(),
                    });
                    self.metrics.set_buffer_peak(self.dsml_buffer.len() as u64);
                    self.dsml_buffer.clear();
                    self.in_dsml = false;
                } else if self.dsml_buffer.contains(CALLS_CLOSE) {
                    let whitelist: Vec<&str> = self
                        .tools_whitelist
                        .as_ref()
                        .map(|w| w.iter().map(|s| s.as_str()).collect())
                        .unwrap_or_default();
                    let result = parse_content(
                        &self.dsml_buffer,
                        Some(&whitelist),
                        &self.config,
                    );

                    for invoke in &result.invokes {
                        let mut id = String::new();
                        for _ in 0..self.config.id_retry_limit {
                            let candidate = format!(
                                "call_{}",
                                &uuid::Uuid::new_v4().simple().to_string()[..24]
                            );
                            if self.id_set.insert(candidate.clone()) {
                                id = candidate;
                                break;
                            }
                        }
                        if id.is_empty() {
                            self.metrics.record_degrade(DegradeReason::IdConflict);
                            tracing::warn!(
                                request_id = %self.request_id,
                                event = "degrade",
                                reason = "id_conflict",
                                "流式 id 生成冲突超限",
                            );
                            continue;
                        }
                        events.push(DeltaEvent::ToolCallStart {
                            index: self.current_index,
                            id: id.clone(),
                            name: invoke.name.clone(),
                        });

                        let mut args_map = serde_json::Map::new();
                        for param in &invoke.parameters {
                            args_map.insert(param.name.clone(), serde_json::json!(param.value));
                        }
                        let args = serde_json::to_string(&serde_json::Value::Object(args_map))
                            .unwrap_or_else(|_| "{}".to_string());
                        events.push(DeltaEvent::ToolCallArguments {
                            index: self.current_index,
                            arguments_chunk: args,
                        });
                        self.current_index += 1;
                    }

                    for de in &result.degrade_events {
                        events.push(DeltaEvent::Degrade {
                            reason: de.reason,
                            snippet: de.raw_snippet.clone(),
                        });
                    }

                    if !result.plain_text.is_empty() {
                        events.push(DeltaEvent::Content(result.plain_text));
                    }

                    self.dsml_buffer.clear();
                    self.in_dsml = false;
                }
            }
        }


        events
    }

    /// 流结束收尾：buffer 非空时降级输出。
    pub fn finalize(&mut self) -> Vec<DeltaEvent> {
        let mut events = Vec::new();
        if !self.dsml_buffer.is_empty() {
            events.push(DeltaEvent::Content(self.dsml_buffer.clone()));
            events.push(DeltaEvent::Degrade {
                reason: DegradeReason::UnclosedBlock,
                snippet: self.dsml_buffer.chars().take(256).collect(),
            });
            self.dsml_buffer.clear();
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_parser() -> DsmlStreamParser {
        DsmlStreamParser::new(
            DsmlConfig::default(),
            Some(vec!["bash".to_string()]),
            DsmlMetrics::register(),
            "test".into(),
        )
    }

    #[test]
    fn test_plain_text_passthrough() {
        let mut p = mk_parser();
        let events = p.feed("hello world");
        assert_eq!(events.len(), 1);
        match &events[0] {
            DeltaEvent::Content(t) => assert_eq!(t, "hello world"),
            _ => panic!("expected Content"),
        }
    }

    #[test]
    fn test_no_whitelist_passthrough() {
        let mut p = DsmlStreamParser::new(
            DsmlConfig::default(),
            None,
            DsmlMetrics::register(),
            "test".into(),
        );
        let events = p.feed("anything");
        assert_eq!(events.len(), 1);
        match &events[0] {
            DeltaEvent::Content(t) => assert_eq!(t, "anything"),
            _ => panic!("expected Content"),
        }
    }
}