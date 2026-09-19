//! SSE 流式处理
//!
//! 将上游 SSE 流式响应实时归一化为 OpenAI chunk 格式：
//!
//! - `openai` 直通：原样转发，同时监测 finish_reason / 流内错误
//! - `anthropic` / `gemini`：转换为 OpenAI chunk 格式
//!
//! ## 设计要点（借鉴 relay-rs）
//!
//! 1. **行缓冲**：TCP 分块与 SSE 事件边界不对齐，残行必须缓存到下一块拼接，
//!    否则被截断的行（典型如收尾的 finish_reason 块）会整行丢失
//! 2. **逐块读取超时**：超过 READ_TIMEOUT 未收到数据则判定上游挂起，
//!    发送错误事件后正常收尾，按"每次读取"计时不会误杀长时间流式生成
//! 3. **错误必须上抛**：流内错误（如上游限流）以 `data: {"error": ...}` 事件
//!    发送给客户端，而不是静默补发 [DONE] 伪装成正常结束
//! 4. **收尾兜底**：流结束但未收到 finish_reason 时补发 stop 块；
//!    既无内容又无 finish_reason 时发送明确错误事件

use actix_web::web::Bytes;
use futures::{Stream, StreamExt};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// 归一化后的 OpenAI 格式 SSE 流（错误以事件形式内嵌，不会以 Err 终止）
pub type SseStream = Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + 'static>>;
/// 上游响应字节流（awc PayloadError）
pub type UpstreamStream = Pin<Box<dyn Stream<Item = Result<Bytes, awc::error::PayloadError>> + 'static>>;
/// 流式逐块读取超时
const READ_TIMEOUT: Duration = Duration::from_secs(120);
/// 行缓冲上限，防止异常上游导致内存无限增长
const MAX_BUFFER_SIZE: usize = 8 * 1024 * 1024;

/// 流式响应收尾时捕获到的 token 用量
#[derive(Debug, Default, Clone, Copy)]
pub struct StreamUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// 缓存创建 Token 数（Anthropic prompt caching）
    pub cache_creation: i64,
    /// 缓存读取 Token 数（Anthropic prompt caching）
    pub cache_read: i64,
    /// 推理 Token 数（OpenAI o1 系列 reasoning_tokens）
    pub reasoning: i64,
    /// token 数是否为估算值（上游未在流中返回 usage 时按内容估算）
    pub estimated: bool,
}

/// 非流式响应提取的用量
#[derive(Debug, Default, Clone, Copy)]
pub struct UsageExtract {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation: i64,
    pub cache_read: i64,
    pub reasoning: i64,
    pub cost: f64,
    pub estimated: bool,
}

/// 按文本内容粗略估算 token 数。
///
/// 估算规则：CJK 字符（中日韩）约 1 字 1 token，其余字符约 4 字符 1 token，
/// 向上取整且至少为 1。仅在上游未返回 usage 时作为兜底使用。
///
/// # 参数
/// - `text`：累计的文本内容
///
/// # 返回
/// 估算的 token 数
pub fn estimate_tokens_from_text(text: &str) -> i64 {
    let mut cjk = 0i64;
    let mut other = 0i64;
    for ch in text.chars() {
        // CJK 统一表意文字、扩展A、兼容表意文字、谚文、假名
        let is_cjk = matches!(ch as u32,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF
            | 0x3040..=0x30FF | 0xAC00..=0xD7AF);
        if is_cjk {
            cjk += 1;
        } else if !ch.is_whitespace() {
            other += 1;
        }
    }
    if cjk == 0 && other == 0 {
        return 0;
    }
    // 其他字符按 4 字符 1 token 折算，与 CJK 相加后向上取整
    (cjk + (other + 3) / 4).max(1)
}

/// 流结束时的用量落库回调。第二个参数为是否成功（false 表示流内含 error 或网络异常）。
pub type UsageCallback = Arc<dyn Fn(StreamUsage, bool) + Send + Sync>;

/// 将归一化 SSE 流包装为 OpenAI 兼容的流式 HTTP 响应
pub fn sse_http_response(stream: SseStream) -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .body(actix_web::body::BodyStream::new(stream))
}

/// 入口：根据源格式分发，返回归一化为 OpenAI chunk 格式的 SSE 流
///
/// # 参数
/// - `upstream`：上游 HTTP 响应
/// - `source_format`：源格式（"openai" / "anthropic" / "gemini"）
///
/// # 返回
/// 归一化为 OpenAI chunk 格式的 SSE 流
pub async fn stream_sse_response(
    upstream: UpstreamStream,
    source_format: &str,
    usage_cb: Option<UsageCallback>,
) -> SseStream {
    match source_format {
        "anthropic" => build_stream(upstream, AnthropicToOpenai::new(), usage_cb),
        "gemini" => build_stream(upstream, GeminiToOpenai::new(), usage_cb),
        _ => build_stream(upstream, OpenAiPassthrough::new(), usage_cb),
    }
}

/// OpenAI 风格的流内错误事件
fn error_event(message: &str) -> String {
    format!(
        "data: {}\n\n",
        json!({"error": {"message": message, "type": "api_error"}})
    )
}

/// 将字节追加到缓冲区，超限则返回 false。
fn push_lossy(buffer: &mut String, bytes: &[u8]) -> bool {
    if buffer.len() + bytes.len() > MAX_BUFFER_SIZE {
        return false;
    }
    buffer.push_str(&String::from_utf8_lossy(bytes));
    true
}

/// 缓冲区溢出时的错误输出。
fn buffer_overflow() -> String {
    let mut out = error_event("SSE 缓冲区溢出，已丢弃当前数据");
    out.push_str("data: [DONE]\n\n");
    out
}

/// 从缓冲区中逐行提取完整行（以 \n 分隔），残留部分保留在缓冲区。
fn take_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(pos) = buffer.find('\n') {
        let line: String = buffer.drain(..pos + 1).collect();
        lines.push(line);
    }
    lines
}

// =============================================================================
// 通用流构建：行缓冲解析器 + 逐块读取超时
// =============================================================================

/// SSE 解析器 trait
///
/// 定义喂入数据块、收尾和错误处理的接口，
/// 不同格式的解析器各自实现此 trait。
trait SseParser: Send + 'static {
    /// 喂入一个网络数据块，返回待发送的输出（可为空）
    fn feed(&mut self, bytes: &[u8]) -> String;
    /// 上游流正常结束时的收尾输出
    fn finish(&mut self) -> String;
    /// 流内是否出现过 error（用于判定请求是否真正成功）
    fn has_error(&self) -> bool {
        false
    }
    /// 捕获到的 token 用量（流结束后读取）
    fn usage(&self) -> StreamUsage {
        StreamUsage::default()
    }
    /// 上游网络错误时的输出
    fn upstream_error(&mut self, msg: &str) -> String {
        let mut out = error_event(&format!("上游流读取错误: {}", msg));
        out.push_str("data: [DONE]\n\n");
        out
    }
    /// 逐块读取超时的输出
    fn read_timeout(&mut self) -> String {
        let mut out = error_event(&format!(
            "上游流式读取超时（{} 秒未收到数据），已终止",
            READ_TIMEOUT.as_secs()
        ));
        out.push_str("data: [DONE]\n\n");
        out
    }
}

// =============================================================================
// 通用流构建：行缓冲解析器 + 逐块读取超时
// =============================================================================

/// 流处理状态
///
/// 持有上游字节流、解析器和完成标志。
struct StreamState<P> {
    /// 上游响应字节流
    inner: UpstreamStream,
    /// SSE 解析器
    parser: P,
    /// 流是否已结束
    done: bool,
}

/// 构建归一化 SSE 流
///
/// 使用 `futures::stream::unfold` 创建异步流，循环读取上游数据块，
/// 通过解析器转换后输出。每次读取应用逐块超时保护。
/// 流终止（正常结束/网络错误/超时）时回调捕获到的 token 用量。
fn build_stream<P: SseParser>(
    upstream: UpstreamStream,
    parser: P,
    usage_cb: Option<UsageCallback>,
) -> SseStream {
    let state = StreamState {
        inner: upstream,
        parser,
        done: false,
    };
    let s = futures::stream::unfold((state, usage_cb), |(mut st, cb)| async move {
        loop {
            // 流已结束，不再产出
            if st.done {
                return None;
            }
            // 逐块读取超时保护
            match tokio::time::timeout(READ_TIMEOUT, st.inner.next()).await {
                Ok(Some(Ok(bytes))) => {
                    // 喂入解析器，获取转换后的输出
                    let out = st.parser.feed(&bytes);
                    if !out.is_empty() {
                        return Some((Ok(Bytes::from(out)), (st, cb)));
                    }
                    // 无完整事件的残块，继续等下一个数据块
                }
                Ok(Some(Err(e))) => {
                    // 上游网络错误：回调已捕获的部分用量后终止
                    st.done = true;
                    let out = st.parser.upstream_error(&e.to_string());
                    if let Some(cb) = cb.as_ref() {
                        cb(st.parser.usage(), false);
                    }
                    return Some((Ok(Bytes::from(out)), (st, cb)));
                }
                Ok(None) => {
                    // 上游流正常结束：收尾并回调用量
                    st.done = true;
                    let out = st.parser.finish();
                    let ok = !st.parser.has_error();
                    if let Some(cb) = cb.as_ref() {
                        cb(st.parser.usage(), ok);
                    }
                    return Some((Ok(Bytes::from(out)), (st, cb)));
                }
                Err(_) => {
                    // 逐块读取超时：回调已捕获的部分用量后终止
                    st.done = true;
                    let out = st.parser.read_timeout();
                    if let Some(cb) = cb.as_ref() {
                        cb(st.parser.usage(), false);
                    }
                    return Some((Ok(Bytes::from(out)), (st, cb)));
                }
            }
        }
    });
    Box::pin(s)
}

/// OpenAI 直通解析器
///
/// 原样转发上游 SSE 数据，同时监测 finish_reason 和内容，
/// 用于收尾兜底（补发 stop 块或错误事件）。
struct OpenAiPassthrough {
    /// 行缓冲区
    buffer: String,
    /// 是否已见到内容
    saw_content: bool,
    /// 流内是否出现过 error
    has_error: bool,
    /// 是否已收到 finish_reason
    saw_finish: bool,
    /// 是否已收到 [DONE] 标记
    saw_done: bool,
    /// 输入 token 数（来自 usage 收尾块）
    input_tokens: i64,
    /// 输出 token 数（来自 usage 收尾块）
    output_tokens: i64,
    /// 推理 token 数（来自 usage.completion_tokens_details.reasoning_tokens）
    reasoning_tokens: i64,
    /// 累计的输出文本内容（content / reasoning_content），用于无 usage 时估算
    content_text: String,
}

impl OpenAiPassthrough {
    /// 创建 OpenAI 直通解析器
    fn new() -> Self {
        Self {
            buffer: String::new(),
            saw_content: false,
            has_error: false,
            saw_finish: false,
            saw_done: false,
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            content_text: String::new(),
        }
    }
}

impl SseParser for OpenAiPassthrough {
    fn feed(&mut self, bytes: &[u8]) -> String {
        // 缓冲区超限保护
        if !push_lossy(&mut self.buffer, bytes) {
            self.buffer.clear();
            return buffer_overflow();
        }

        let mut out = String::new();
        for line in take_lines(&mut self.buffer) {
            if line.is_empty() {
                out.push('\n');
                continue;
            }
            // 原样透传
            out.push_str(&line);
            out.push('\n');

            // 监测（不修改内容）
            let data = line
                .strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"));
            let Some(data) = data else { continue };
            if data == "[DONE]" {
                self.saw_done = true;
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };
            // 检测流内 error
            if v.get("error").is_some() {
                self.has_error = true;
            }
            // 监测 finish_reason 和内容
            if let Some(choice) = v.get("choices").and_then(|c| c.get(0)) {
                if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    self.saw_finish = true;
                    let _ = fr;
                }
                if let Some(delta) = choice.get("delta") {
                    // 累计输出文本（content 与 reasoning_content 都计入）
                    if let Some(c) = delta.get("content").and_then(|c| c.as_str())
                        && !c.is_empty() {
                            self.saw_content = true;
                            self.content_text.push_str(c);
                        }
                    if let Some(c) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
                        self.content_text.push_str(c);
                    }
                }
            }
            // 捕获收尾 usage 块（需上游开启 stream_options.include_usage）
            if let Some(usage) = v.get("usage").and_then(|u| u.as_object()) {
                if let Some(t) = usage.get("prompt_tokens").and_then(|t| t.as_i64()) {
                    self.input_tokens = t;
                }
                if let Some(t) = usage.get("completion_tokens").and_then(|t| t.as_i64()) {
                    self.output_tokens = t;
                }
                // OpenAI o1 系列：completion_tokens_details.reasoning_tokens
                if let Some(t) = usage
                    .get("completion_tokens_details")
                    .and_then(|d| d.get("reasoning_tokens"))
                    .and_then(|t| t.as_i64())
                {
                    self.reasoning_tokens = t;
                }
            }
        }
        out
    }

    fn has_error(&self) -> bool {
        self.has_error
    }

    fn finish(&mut self) -> String {
        let mut out = String::new();
        if !self.saw_finish {
            if self.saw_content {
                // 流有内容但未收到 finish_reason：补发 stop 块，正常收尾
                log::warn!("SSE 流结束但未收到 finish_reason，补发 stop 块");
                out.push_str(&format!(
                    "data: {}\n\n",
                    json!({
                        "object": "chat.completion.chunk",
                        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
                    })
                ));
            } else {
                // 流既无内容也无 finish_reason：发明确错误，
                // 避免客户端误判为"正常完成但无内容"
                log::error!("SSE 流异常结束：无内容且无 finish_reason");
                out.push_str(&error_event(
                    "上游返回了空响应（无内容且未正常结束），可能被限流或服务异常，请重试",
                ));
            }
        }
        // 确保发送 [DONE] 收尾标记
        if !self.saw_done {
            out.push_str("data: [DONE]\n\n");
        }
        out
    }

    fn usage(&self) -> StreamUsage {
        // 上游未返回 usage 收尾块：按累计输出内容估算（估算值由引擎回填输入）
        if self.input_tokens == 0 && self.output_tokens == 0 && !self.content_text.is_empty() {
            return StreamUsage {
                input_tokens: 0,
                output_tokens: estimate_tokens_from_text(&self.content_text),
                estimated: true,
                ..Default::default()
            };
        }
        StreamUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            reasoning: self.reasoning_tokens,
            estimated: false,
            ..Default::default()
        }
    }
}

// =============================================================================
// Anthropic → OpenAI 转换解析器
// =============================================================================

/// Anthropic → OpenAI SSE 转换解析器
///
/// 将 Anthropic Messages 流式事件转换为 OpenAI chat.completion.chunk 格式，
/// 支持 text、thinking、tool_use 内容块的完整转换。
struct AnthropicToOpenai {
    /// 行缓冲区
    buffer: String,
    /// 响应 ID
    id: String,
    /// 模型名
    model: String,
    /// 是否已发送首个 chunk
    started: bool,
    /// 是否已见到内容
    saw_content: bool,
    /// 流内是否出现过 error
    has_error: bool,
    /// finish_reason（已映射为 OpenAI 格式）
    finish_reason: Option<String>,
    /// 输入 token 计数（来自 message_start）
    input_tokens: i64,
    /// 输出 token 计数
    output_tokens: i64,
    /// 缓存创建 token 数（Anthropic prompt caching，来自 message_start）
    cache_creation: i64,
    /// 缓存读取 token 数（Anthropic prompt caching，来自 message_start）
    cache_read: i64,
    /// 累计的输出文本内容（text / thinking），用于无 usage 时估算
    content_text: String,
    /// 内容块索引 → 是否为 tool_use 块
    block_is_tool: Vec<bool>,
    /// tool_use 块索引 → OpenAI tool_calls 中的 index
    tool_index: Vec<usize>,
}

impl AnthropicToOpenai {
    /// 创建 Anthropic → OpenAI 转换解析器
    fn new() -> Self {
        Self {
            buffer: String::new(),
            id: "chatcmpl-anthropic".to_string(),
            model: "anthropic".to_string(),
            started: false,
            saw_content: false,
            has_error: false,
            finish_reason: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_creation: 0,
            cache_read: 0,
            content_text: String::new(),
            block_is_tool: Vec::new(),
            tool_index: Vec::new(),
        }
    }

    /// 构建一个 OpenAI chunk 格式的 SSE 事件
    fn chunk(&self, delta: serde_json::Value, finish: Option<&str>) -> String {
        format!(
            "data: {}\n\n",
            json!({
                "id": self.id,
                "object": "chat.completion.chunk",
                "created": chrono::Utc::now().timestamp(),
                "model": self.model,
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]
            })
        )
    }

    /// 处理单个 Anthropic SSE 事件（一个事件内含 data 行）
    fn handle_event(&mut self, event: &str) -> String {
        let mut out = String::new();
        for line in event.lines() {
            let Some(data) = line
                .strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
            else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };

            let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match event_type {
                "message_start" => {
                    // 提取响应 ID、模型名与输入 token 数
                    if let Some(msg) = v.get("message") {
                        self.id = msg
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("chatcmpl-anthropic")
                            .to_string();
                        self.model = msg
                            .get("model")
                            .and_then(|m| m.as_str())
                            .unwrap_or("anthropic")
                            .to_string();
                        if let Some(usage) = msg.get("usage") {
                            if let Some(t) = usage.get("input_tokens").and_then(|t| t.as_i64()) {
                                self.input_tokens = t;
                            }
                            if let Some(t) = usage.get("cache_creation_input_tokens").and_then(|t| t.as_i64()) {
                                self.cache_creation = t;
                            }
                            if let Some(t) = usage.get("cache_read_input_tokens").and_then(|t| t.as_i64()) {
                                self.cache_read = t;
                            }
                        }
                    }
                    // 发送首个 assistant 角色 chunk
                    if !self.started {
                        self.started = true;
                        out.push_str(&self.chunk(json!({"role": "assistant", "content": ""}), None));
                    }
                }
                "content_block_start" => {
                    // 内容块开始：text 或 tool_use
                    let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let block = v.get("content_block").cloned().unwrap_or(json!({}));
                    let is_tool = block.get("type").and_then(|t| t.as_str()) == Some("tool_use");
                    // 扩展块索引数组
                    while self.block_is_tool.len() <= index {
                        self.block_is_tool.push(false);
                        self.tool_index.push(0);
                    }
                    if is_tool {
                        // tool_use 块：映射为 OpenAI tool_calls
                        self.block_is_tool[index] = true;
                        self.tool_index[index] = self.block_is_tool.iter().filter(|t| **t).count().saturating_sub(1);
                        let tc = json!({
                            "index": self.tool_index[index],
                            "id": block.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                            "type": "function",
                            "function": {
                                "name": block.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                "arguments": ""
                            }
                        });
                        out.push_str(&self.chunk(json!({"tool_calls": [tc]}), None));
                    }
                }
                "content_block_delta" => {
                    // 内容块增量
                    let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let delta = v.get("delta").cloned().unwrap_or(json!({}));
                    match delta.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                        "text_delta" => {
                            // 文本增量
                            let text = delta.get("text").and_then(|t| t.as_str()).unwrap_or("");
                            if !text.is_empty() {
                                self.saw_content = true;
                                self.content_text.push_str(text);
                                out.push_str(&self.chunk(json!({"content": text}), None));
                            }
                        }
                        "thinking_delta" => {
                            // 思考增量 → reasoning_content
                            let thinking = delta.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                            if !thinking.is_empty() {
                                self.content_text.push_str(thinking);
                                out.push_str(&self.chunk(json!({"reasoning_content": thinking}), None));
                            }
                        }
                        "input_json_delta" => {
                            // tool_use 参数增量
                            let partial = delta.get("partial_json").and_then(|p| p.as_str()).unwrap_or("");
                            if !partial.is_empty() && self.block_is_tool.get(index).copied().unwrap_or(false) {
                                let tc = json!({
                                    "index": self.tool_index[index],
                                    "function": {"arguments": partial}
                                });
                                out.push_str(&self.chunk(json!({"tool_calls": [tc]}), None));
                            }
                        }
                        _ => {}
                    }
                }
                "message_delta" => {
                    // 消息级增量：stop_reason 和 usage
                    if let Some(delta) = v.get("delta") {
                        let stop = delta.get("stop_reason").and_then(|s| s.as_str());
                        // 映射 Anthropic stop_reason → OpenAI finish_reason
                        let mapped = match stop {
                            Some("end_turn") | None => "stop",
                            Some("tool_use") => "tool_calls",
                            Some("max_tokens") => "length",
                            Some(other) => other,
                        };
                        self.finish_reason = Some(mapped.to_string());
                    }
                    if let Some(u) = v.get("usage") {
                        self.output_tokens = u.get("output_tokens").and_then(|t| t.as_i64()).unwrap_or(0);
                    }
                    // 发送 finish_reason chunk
                    let mut chunk = json!({
                        "choices": [{"index": 0, "delta": {}, "finish_reason": mapped_finish(self.finish_reason.as_deref())}]
                    });
                    if self.output_tokens > 0 {
                        chunk["usage"] = json!({"completion_tokens": self.output_tokens});
                    }
                    out.push_str(&format!("data: {}\n\n", chunk));
                }
                "error" => {
                    // 流内错误事件
                    self.has_error = true;
                    let err = v.get("error").cloned().unwrap_or(json!({}));
                    let msg = match err.get("message").and_then(|m| m.as_str()) {
                        Some(m) => m.to_string(),
                        None => err.to_string(),
                    };
                    out.push_str(&error_event(&format!("上游流内错误: {}", msg)));
                }
                _ => {}
            }
        }
        out
    }
}

/// 将 finish_reason 映射为 OpenAI 格式（None 默认为 "stop"）
fn mapped_finish(reason: Option<&str>) -> &str {
    reason.unwrap_or("stop")
}

impl SseParser for AnthropicToOpenai {
    fn feed(&mut self, bytes: &[u8]) -> String {
        // 缓冲区超限保护
        if !push_lossy(&mut self.buffer, bytes) {
            self.buffer.clear();
            return buffer_overflow();
        }

        let mut out = String::new();
        // Anthropic SSE 事件以空行分隔
        while let Some(pos) = self.buffer.find("\n\n") {
            let event: String = self.buffer.drain(..pos + 2).collect();
            let event = event.trim();
            if event.is_empty() {
                continue;
            }
            out.push_str(&self.handle_event(event));
        }
        out
    }

    fn has_error(&self) -> bool {
        self.has_error
    }

    fn finish(&mut self) -> String {
        // 处理残留在缓冲区的最后一个事件
        let mut out = String::new();
        let rest = std::mem::take(&mut self.buffer);
        let rest = rest.trim().to_string();
        if !rest.is_empty() {
            out.push_str(&self.handle_event(&rest));
        }

        // 收尾兜底：未收到 stop_reason 时补发
        if self.finish_reason.is_none() {
            if self.saw_content {
                log::warn!("Anthropic 流结束但未收到 stop_reason，补发 stop 块");
                out.push_str(&self.chunk(json!({}), Some("stop")));
            } else {
                out.push_str(&error_event(
                    "上游返回了空响应（无内容且未正常结束），可能被限流或服务异常，请重试",
                ));
            }
        }
        out.push_str("data: [DONE]\n\n");
        out
    }

    fn usage(&self) -> StreamUsage {
        // 上游未回传 output_tokens（message_delta 缺失 usage）：按累计输出内容估算
        if self.output_tokens == 0 && !self.content_text.is_empty() {
            return StreamUsage {
                input_tokens: self.input_tokens,
                output_tokens: estimate_tokens_from_text(&self.content_text),
                cache_creation: self.cache_creation,
                cache_read: self.cache_read,
                estimated: true,
                ..Default::default()
            };
        }
        StreamUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_creation: self.cache_creation,
            cache_read: self.cache_read,
            estimated: false,
            ..Default::default()
        }
    }
}

// =============================================================================
// Gemini → OpenAI 转换解析器
// =============================================================================

/// Gemini → OpenAI SSE 转换解析器
///
/// 将 Gemini streamGenerateContent 响应转换为 OpenAI chat.completion.chunk 格式。
struct GeminiToOpenai {
    /// 行缓冲区
    buffer: String,
    /// 响应 ID
    id: String,
    /// 模型名
    model: String,
    /// 是否已发送首个 chunk
    started: bool,
    /// 是否已见到内容
    saw_content: bool,
    /// 流内是否出现过 error
    has_error: bool,
    /// 是否已收到 finishReason
    saw_finish: bool,
    /// 输入 token 数（来自 usageMetadata.promptTokenCount）
    input_tokens: i64,
    /// 输出 token 数（来自 usageMetadata.candidatesTokenCount）
    output_tokens: i64,
    /// 累计的输出文本内容，用于无 usageMetadata 时估算
    content_text: String,
}

impl GeminiToOpenai {
    /// 创建 Gemini → OpenAI 转换解析器
    fn new() -> Self {
        Self {
            buffer: String::new(),
            id: format!(
                "chatcmpl-gemini-{}",
                &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]
            ),
            model: "gemini".to_string(),
            started: false,
            saw_content: false,
            has_error: false,
            saw_finish: false,
            input_tokens: 0,
            output_tokens: 0,
            content_text: String::new(),
        }
    }

    /// 构建一个 OpenAI chunk 格式的 SSE 事件
    fn chunk(&self, delta: serde_json::Value, finish: Option<&str>) -> String {
        format!(
            "data: {}\n\n",
            json!({
                "id": self.id,
                "object": "chat.completion.chunk",
                "created": chrono::Utc::now().timestamp(),
                "model": self.model,
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]
            })
        )
    }
}

impl SseParser for GeminiToOpenai {
    fn feed(&mut self, bytes: &[u8]) -> String {
        // 缓冲区超限保护
        if !push_lossy(&mut self.buffer, bytes) {
            self.buffer.clear();
            return buffer_overflow();
        }

        let mut out = String::new();
        for line in take_lines(&mut self.buffer) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 兼容被引号包裹的 JSON 字符串行
            let json_str = if line.starts_with('"') && line.ends_with('"') && line.len() > 2 {
                let inner = &line[1..line.len() - 1];
                inner.replace("\\\"", "\"").replace("\\\\", "\\")
            } else {
                line.to_string()
            };

            let Ok(event) = serde_json::from_str::<serde_json::Value>(&json_str) else {
                continue;
            };

            // 检查流内错误
            if let Some(err) = event.get("error")
                && !err.is_null() {
                    self.has_error = true;
                    let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or(&json_str);
                    out.push_str(&error_event(&format!("上游流内错误: {}", msg)));
                    continue;
                }

            // 首个事件：发送 assistant 角色 chunk，提取模型版本
            if !self.started {
                self.started = true;
                if let Some(m) = event.get("modelVersion").and_then(|m| m.as_str()) {
                    self.model = m.to_string();
                }
                out.push_str(&self.chunk(json!({"role": "assistant", "content": ""}), None));
            }

            // 捕获 usageMetadata（promptTokenCount / candidatesTokenCount）
            if let Some(meta) = event.get("usageMetadata") {
                if let Some(t) = meta.get("promptTokenCount").and_then(|t| t.as_i64()) {
                    self.input_tokens = t;
                }
                if let Some(t) = meta.get("candidatesTokenCount").and_then(|t| t.as_i64()) {
                    self.output_tokens = t;
                }
            }

            // 处理 candidates
            let Some(candidates) = event.get("candidates").and_then(|c| c.as_array()) else {
                continue;
            };
            for candidate in candidates {
                // 映射 Gemini finishReason → OpenAI finish_reason
                let finish_reason = candidate
                    .get("finishReason")
                    .and_then(|f| f.as_str())
                    .map(|r| if r == "STOP" { "stop" } else { r });

                // 提取文本内容
                if let Some(parts) = candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str())
                            && !text.is_empty() {
                                self.saw_content = true;
                                self.content_text.push_str(text);
                                out.push_str(&self.chunk(json!({"content": text}), None));
                            }
                    }
                }

                // 发送 finish_reason
                if let Some(fr) = finish_reason {
                    self.saw_finish = true;
                    out.push_str(&self.chunk(json!({}), Some(fr)));
                }
            }
        }
        out
    }

    fn has_error(&self) -> bool {
        self.has_error
    }

    fn finish(&mut self) -> String {
        let mut out = String::new();
        // 收尾兜底
        if !self.saw_finish {
            if self.saw_content {
                log::warn!("Gemini 流结束但未收到 finishReason，补发 stop 块");
                out.push_str(&self.chunk(json!({}), Some("stop")));
            } else if self.started {
                out.push_str(&error_event(
                    "上游返回了空响应（无内容且未正常结束），可能被限流或服务异常，请重试",
                ));
            }
        }
        out.push_str("data: [DONE]\n\n");
        out
    }

    fn usage(&self) -> StreamUsage {
        // 上游未回传 usageMetadata：按累计输出内容估算
        if self.input_tokens == 0 && self.output_tokens == 0 && !self.content_text.is_empty() {
            return StreamUsage {
                input_tokens: 0,
                output_tokens: estimate_tokens_from_text(&self.content_text),
                estimated: true,
                ..Default::default()
            };
        }
        StreamUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            estimated: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯 CJK 文本：1 字 1 token
    #[test]
    fn estimate_tokens_cjk() {
        assert_eq!(estimate_tokens_from_text("你好世界"), 4);
        // 5 个 CJK 字符 → 5 token
        assert_eq!(estimate_tokens_from_text("你好世界呀"), 5);
    }

    /// 纯英文文本：约 4 字符 1 token
    #[test]
    fn estimate_tokens_ascii() {
        // 8 个非空白字符 → 2 token
        assert_eq!(estimate_tokens_from_text("abcdefgh"), 2);
        // 5 个字符 → 向上取整 2 token
        assert_eq!(estimate_tokens_from_text("abcde"), 2);
    }

    /// 混合文本：CJK 与英文分别计数后求和
    #[test]
    fn estimate_tokens_mixed() {
        // 2 个 CJK（2 token）+ 4 个英文字符（1 token）= 3 token
        assert_eq!(estimate_tokens_from_text("你好abcd"), 3);
    }

    /// 空文本与纯空白：返回 0
    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens_from_text(""), 0);
        assert_eq!(estimate_tokens_from_text("   \n\t"), 0);
    }
}
