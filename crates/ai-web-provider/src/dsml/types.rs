//! DSML 核心数据类型。

use std::fmt;

/// 单次工具调用：invoke 名称 + 参数列表。
#[derive(Debug, Clone, PartialEq)]
pub struct Invoke {
    pub name: String,
    pub parameters: Vec<Parameter>,
}

/// invoke 的单个参数。
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub value: String,
    pub is_string: bool,
}

/// 非流式解析结果：合法 invoke 列表 + 剥离后的普通文本 + 降级事件。
#[derive(Debug, Clone, Default)]
pub struct ParseResult {
    pub invokes: Vec<Invoke>,
    pub plain_text: String,
    pub degrade_events: Vec<DegradeEvent>,
}

/// 降级事件：原因 + 原文片段（≤256 字符，已脱敏）+ 关联 invoke 索引。
#[derive(Debug, Clone)]
pub struct DegradeEvent {
    pub reason: DegradeReason,
    pub raw_snippet: String,
    pub invoke_index: Option<usize>,
}

/// 降级原因枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DegradeReason {
    UnclosedBlock,
    InvalidName,
    SerializeFail,
    IdConflict,
    BufferOverflow,
    MemoryOverflow,
    NestedImbalance,
    RegistryMiss,
    DuplicateParam,
}

impl fmt::Display for DegradeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DegradeReason::UnclosedBlock => write!(f, "unclosed_block"),
            DegradeReason::InvalidName => write!(f, "invalid_name"),
            DegradeReason::SerializeFail => write!(f, "serialize_fail"),
            DegradeReason::IdConflict => write!(f, "id_conflict"),
            DegradeReason::BufferOverflow => write!(f, "buffer_overflow"),
            DegradeReason::MemoryOverflow => write!(f, "memory_overflow"),
            DegradeReason::NestedImbalance => write!(f, "nested_imbalance"),
            DegradeReason::RegistryMiss => write!(f, "registry_miss"),
            DegradeReason::DuplicateParam => write!(f, "duplicate_param"),
        }
    }
}

/// 流式增量事件：由 [`super::stream::DsmlStreamParser`] 逐 chunk 产出。
#[derive(Debug, Clone)]
pub enum DeltaEvent {
    /// 普通文本（实时透传，不等待 DSML 解析）
    Content(String),
    /// tool_call 起始：声明 index / id / name
    ToolCallStart { index: usize, id: String, name: String },
    /// tool_call arguments 分片（跨 chunk 累加，相同 index）
    ToolCallArguments { index: usize, arguments_chunk: String },
    /// 降级事件（内容已通过 Content 事件输出，此事件仅记日志指标）
    Degrade { reason: DegradeReason, snippet: String },
}