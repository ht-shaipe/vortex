//! Anthropic Messages 入站端点
//!
//! 让 Claude Code / Claude 桌面端等 Anthropic 协议客户端直连 Vortex 的
//! 多提供商路由（借鉴 relay-rs 的双向协议转换设计）：
//!
//! - 请求：Anthropic 格式 → 内部 OpenAI 格式（`translator::anthropic_request_to_openai_body`）
//! - 非流式响应：OpenAI JSON → Anthropic Messages 响应
//! - 流式响应：归一化 OpenAI chunk 流 → Anthropic SSE 事件序列
//!   （message_start → content_block_start → content_block_delta… →
//!    content_block_stop → message_delta → message_stop）
//!
//! 支持 Function Calling 全链路：入站 tools/tool_choice/tool_result，
//! 出站 tool_use 内容块与 input_json_delta 流式增量。

use crate::db::models::ProxyRequest;
use crate::proxy::engine::ProxyOutput;
use crate::proxy::sse::{sse_http_response, SseStream};
use crate::AppState;
use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// SSE 单行缓冲区上限：8 MB，超过则终止流并返回错误事件
const MAX_BUFFER_SIZE: usize = 8 * 1024 * 1024;

/// 处理 POST /v1/messages 请求，接收 Anthropic 格式消息并路由到多提供商。
///
/// 将 Anthropic 请求体转换为内部 OpenAI 格式后交给代理引擎处理。
/// - 非流式：将引擎返回的 OpenAI JSON 响应转换回 Anthropic Messages 响应格式。
/// - 流式：将归一化 OpenAI SSE chunk 流转换为 Anthropic SSE 事件序列。
///
/// - `state`：应用全局状态
/// - `body`：Anthropic 格式 JSON 请求体
/// - `req`：原始 HTTP 请求（用于提取 `x-api-key` 或 `Authorization` 头）
/// - 返回值：Anthropic 格式响应或 SSE 流；失败时为 Anthropic 风格错误
pub async fn anthropic_messages(
    state: web::Data<Arc<AppState>>,
    body: web::Json<Value>,
    req: HttpRequest,
) -> HttpResponse {
    let body_inner = body.into_inner(); // 取出请求体所有权
    // 保留原始模型名，用于响应中回填 model 字段
    let original_model = body_inner
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // 解析 Anthropic 请求为内部 ProxyRequest
    let request = match parse_anthropic_request(&body_inner, &req) {
        Ok(r) => r,
        Err(e) => return super::anthropic_error_response(&e),
    };

    // 获取代理引擎读锁，处理请求
    let engine = state.proxy_engine.read();
    match engine.handle_request(&state, request).await {
        Ok(ProxyOutput::Response(body)) => {
            // 非流式：上游 OpenAI JSON → Anthropic Messages 响应
            HttpResponse::Ok().json(
                crate::translator::openai_to_anthropic_response(&body, &original_model),
            )
        }
        Ok(ProxyOutput::Stream(stream)) => {
            // 流式：将 OpenAI chunk 流转换为 Anthropic SSE 事件流
            let anthropic_stream = anthropic_stream_from_openai(stream, original_model);
            sse_http_response(anthropic_stream)
        }
        Err(e) => super::anthropic_error_response(&e),
    }
}

/// 将 Anthropic 格式请求体解析为内部 [`ProxyRequest`]。
///
/// 提取 `model`（必填）、`stream` 等字段，通过 [`anthropic_request_to_openai_body`]
/// 将 Anthropic 请求体转换为 OpenAI 格式。API 密钥优先从 `x-api-key` 头提取，
/// 回退到 `Authorization: Bearer` 头。
///
/// - `body`：Anthropic 格式 JSON 请求体
/// - `req`：HTTP 请求，用于提取鉴权头
/// - 返回值：解析成功的 [`ProxyRequest`] 或 [`AppError`](crate::error::AppError)
fn parse_anthropic_request(
    body: &Value,
    req: &HttpRequest,
) -> crate::error::Result<ProxyRequest> {
    // 提取模型名（必填字段）
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("model is required".into()))?
        .to_string();

    // 提取流式标志，默认 false
    let stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    // 将 Anthropic 请求体转换为 OpenAI 格式
    let mut openai_body = crate::translator::anthropic_request_to_openai_body(body);
    openai_body["model"] = json!(model); // 回填模型名
    openai_body["stream"] = json!(stream); // 回填流式标志

    // Anthropic 客户端用 x-api-key，兼容 Authorization Bearer
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        });

    // 构造内部代理请求
    Ok(ProxyRequest {
        model,
        messages: openai_body,
        stream,
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        max_tokens: body.get("max_tokens").and_then(|v| v.as_i64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        api_key,
        source_format: "anthropic".to_string(), // 标记来源格式为 Anthropic
        extra: json!({}),
    })
}

// =============================================================================
// OpenAI chunk 流 → Anthropic SSE 事件流
// =============================================================================

/// 构造一条 SSE 事件字符串（`event: <name>\ndata: <data>\n\n`）。
///
/// - `name`：SSE 事件类型名称
/// - `data`：事件数据（JSON 值）
/// - 返回值：符合 SSE 协议格式的事件字符串
fn sse_event(name: &str, data: Value) -> String {
    format!("event: {}\ndata: {}\n\n", name, data)
}

/// 构造一条 Anthropic 风格的 SSE error 事件。
///
/// - `message`：错误描述文本
/// - 返回值：`event: error\ndata: {...}\n\n` 格式字符串
fn anthropic_error_event(message: &str) -> String {
    sse_event(
        "error",
        json!({"type": "error", "error": {"type": "api_error", "message": message}}),
    )
}

/// 将归一化 OpenAI SSE 流转换为 Anthropic SSE 事件流。
/// 输入流已具备逐块读取超时与错误事件，此处只做格式转换。
///
/// 使用 `futures::stream::unfold` 构造有状态异步迭代器，内部维护
/// [`OpenAiToAnthropic`] 转换器状态。每个输入字节块经 [`feed`](OpenAiToAnthropic::feed)
/// 处理后产出 Anthropic 事件；流结束时调用 [`finish`](OpenAiToAnthropic::finish) 收尾。
///
/// - `input`：归一化 OpenAI SSE 字节流
/// - `model`：原始模型名，用于 message_start 事件
/// - 返回值：Anthropic SSE 事件字节流
fn anthropic_stream_from_openai(input: SseStream, model: String) -> SseStream {
    let conv = OpenAiToAnthropic::new(model); // 创建转换器
    let s = futures::stream::unfold(
        (input, conv, false), // (输入流, 转换器, 是否已结束)
        |(mut input, mut conv, done)| async move {
            loop {
                if done {
                    return None; // 已结束，停止迭代
                }
                match input.next().await {
                    Some(Ok(bytes)) => {
                        // 喂入字节块，获取转换后的事件
                        let out = conv.feed(&bytes);
                        if !out.is_empty() {
                            return Some((Ok(actix_web::web::Bytes::from(out)), (input, conv, false)));
                        }
                        // 拋块无完整事件，继续
                    }
                    Some(Err(_)) => {
                        // 不会发生（错误以事件形式内嵌），保险处理
                        let out = anthropic_error_event("上游流读取错误");
                        return Some((Ok(actix_web::web::Bytes::from(out)), (input, conv, true)));
                    }
                    None => {
                        // 输入流结束，发送收尾事件
                        let out = conv.finish();
                        if out.is_empty() {
                            return None;
                        }
                        return Some((Ok(actix_web::web::Bytes::from(out)), (input, conv, true)));
                    }
                }
            }
        },
    );
    Box::pin(s)
}

/// OpenAI chunk → Anthropic 事件转换器（行缓冲）
///
/// 维护 SSE 行缓冲区与转换状态机，将逐块到达的 OpenAI streaming chunk
/// 转换为 Anthropic Messages SSE 事件序列。负责管理内容块的生命周期
/// （text 块与 tool_use 块的开启/增量/关闭）以及消息级别的收尾。
struct OpenAiToAnthropic {
    /// SSE 行缓冲区，累积未完成的行
    buffer: String,
    /// 当前模型名（从首个 chunk 或入参获取）
    model: String,
    /// 本次消息的唯一 ID（msg_ 前缀 + UUID）
    msg_id: String,
    /// 是否已发送 message_start 事件
    started: bool,
    /// 当前打开的内容块索引
    open_block: Option<usize>,
    /// 下一个待分配的内容块索引
    next_block: usize,
    /// OpenAI tool_calls index → Anthropic 内容块索引
    tool_blocks: HashMap<usize, usize>,
    /// 是否已收到任何文本内容
    saw_content: bool,
    /// 是否已收到 finish_reason
    finished: bool,
    /// 是否已发生错误
    errored: bool,
    /// 停止原因（end_turn / tool_use / max_tokens 等）
    stop_reason: String,
    /// 输入 token 数（来自 usage 收尾块）
    input_tokens: i64,
    /// 输出 token 数（来自 usage 收尾块）
    output_tokens: i64,
}

impl OpenAiToAnthropic {
    /// 创建一个新的转换器实例。
    ///
    /// - `model`：初始模型名，可能被首个 chunk 中的 model 字段覆盖
    fn new(model: String) -> Self {
        Self {
            buffer: String::new(),
            model,
            msg_id: String::new(),
            started: false,
            open_block: None,
            next_block: 0,
            tool_blocks: HashMap::new(),
            saw_content: false,
            finished: false,
            errored: false,
            stop_reason: "end_turn".to_string(), // 默认停止原因
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    /// 喂入一段字节块，返回已完成的 Anthropic SSE 事件字符串。
    ///
    /// 将字节追加到行缓冲区，按换行符分割为完整行，逐行解析 `data:` 前缀的
    /// JSON chunk 并交给 [`handle_chunk`] 处理。若缓冲区超过
    /// [`MAX_BUFFER_SIZE`] 则终止并返回错误事件。
    ///
    /// - `bytes`：输入的字节块
    /// - 返回值：转换后的 Anthropic SSE 事件字符串（可能为空）
    fn feed(&mut self, bytes: &[u8]) -> String {
        // 追加到行缓冲区
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        // 缓冲区超限保护
        if self.buffer.len() > MAX_BUFFER_SIZE {
            self.buffer.clear();
            self.errored = true;
            return anthropic_error_event(&format!(
                "SSE 单行数据超过 {} MB 上限，已终止",
                MAX_BUFFER_SIZE / 1024 / 1024
            ));
        }

        let mut out = String::new();
        // 逐行处理缓冲区中的完整行
        while let Some(pos) = self.buffer.find('\n') {
            if self.errored {
                self.buffer.clear();
                break;
            }
            // 取出一行并去除行尾换行符
            let line: String = self.buffer.drain(..=pos).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            // 提取 data: 前缀后的 JSON 内容
            let Some(data) = line
                .strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
            else {
                continue; // 非 data 行，跳过
            };
            if data == "[DONE]" {
                continue; // 流结束标记，跳过（由 finish 处理收尾）
            }
            // 解析 JSON chunk 并处理
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                out.push_str(&self.handle_chunk(&v));
            }
        }
        out
    }

    /// 处理单个 OpenAI streaming chunk，返回对应的 Anthropic SSE 事件字符串。
    ///
    /// 负责：
    /// - 首次调用时发送 `message_start` 事件。
    /// - 将 `delta.content` 文本增量映射为 `text_delta` 事件。
    /// - 将 `delta.tool_calls` 工具调用增量映射为 `tool_use` 块的开启与 `input_json_delta` 事件。
    /// - 提取 `usage` 中的 token 计数。
    /// - 检测 `finish_reason` 并映射为 Anthropic 停止原因。
    ///
    /// - `v`：单个 OpenAI chunk 的 JSON 值
    /// - 返回值：转换后的 Anthropic SSE 事件字符串（可能为空）
    fn handle_chunk(&mut self, v: &Value) -> String {
        // 流内错误 → Anthropic error 事件
        if let Some(err) = v.get("error") {
            if !err.is_null() {
                self.errored = true;
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown upstream error");
                return anthropic_error_event(&format!("上游流内错误: {}", msg));
            }
        }

        // usage（stream_options include_usage 的收尾块）
        if let Some(u) = v.get("usage").and_then(|u| u.as_object()) {
            self.input_tokens = u
                .get("prompt_tokens")
                .and_then(|t| t.as_i64())
                .unwrap_or(self.input_tokens);
            self.output_tokens = u
                .get("completion_tokens")
                .and_then(|t| t.as_i64())
                .unwrap_or(self.output_tokens);
        }

        // 提取第一个 choice，无则返回空
        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
            return String::new();
        };

        let mut out = String::new();

        // 首次处理：发送 message_start 事件
        if !self.started {
            self.started = true;
            // 生成消息唯一 ID
            self.msg_id = format!(
                "msg_{}",
                uuid::Uuid::new_v4().to_string().replace("-", "")
            );
            // 从 chunk 中更新模型名（若存在且非空）
            if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
                if !m.is_empty() {
                    self.model = m.to_string();
                }
            }
            out.push_str(&sse_event(
                "message_start",
                json!({
                    "type": "message_start",
                    "message": {
                        "id": self.msg_id,
                        "type": "message",
                        "role": "assistant",
                        "model": self.model,
                        "content": [],
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": {"input_tokens": 0, "output_tokens": 0}
                    }
                }),
            ));
        }

        if let Some(delta) = choice.get("delta") {
            // 文本增量 → text 块
            if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                if !text.is_empty() {
                    self.saw_content = true;
                    // 若当前无打开的文本块，则开启新块
                    if self.open_block.is_none() {
                        let idx = self.next_block;
                        self.next_block += 1;
                        out.push_str(&sse_event(
                            "content_block_start",
                            json!({
                                "type": "content_block_start",
                                "index": idx,
                                "content_block": {"type": "text", "text": ""}
                            }),
                        ));
                        self.open_block = Some(idx);
                    }
                    // 发送文本增量事件
                    out.push_str(&sse_event(
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta",
                            "index": self.open_block,
                            "delta": {"type": "text_delta", "text": text}
                        }),
                    ));
                }
            }

            // 工具调用增量 → tool_use 块
            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    let ti = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize; // tool_calls 索引
                    let func = tc.get("function").cloned().unwrap_or(json!({}));

                    // 新工具（携带 id）：开新块
                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                        if !id.is_empty() && !self.tool_blocks.contains_key(&ti) {
                            // 关闭当前打开的块
                            if let Some(ob) = self.open_block.take() {
                                out.push_str(&sse_event(
                                    "content_block_stop",
                                    json!({"type": "content_block_stop", "index": ob}),
                                ));
                            }
                            // 开启新的 tool_use 块
                            let idx = self.next_block;
                            self.next_block += 1;
                            self.tool_blocks.insert(ti, idx); // 记录 tool_calls 索引到块索引的映射
                            out.push_str(&sse_event(
                                "content_block_start",
                                json!({
                                    "type": "content_block_start",
                                    "index": idx,
                                    "content_block": {
                                        "type": "tool_use",
                                        "id": id,
                                        "name": func.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                        "input": {}
                                    }
                                }),
                            ));
                            self.open_block = Some(idx);
                        }
                    }

                    // 参数分片 → input_json_delta
                    if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                        if !args.is_empty() {
                            if let Some(&idx) = self.tool_blocks.get(&ti) {
                                // 若当前打开的块不是目标块，先切换
                                if self.open_block != Some(idx) {
                                    if let Some(ob) = self.open_block.take() {
                                        out.push_str(&sse_event(
                                            "content_block_stop",
                                            json!({"type": "content_block_stop", "index": ob}),
                                        ));
                                    }
                                    self.open_block = Some(idx);
                                }
                                // 发送参数增量事件
                                out.push_str(&sse_event(
                                    "content_block_delta",
                                    json!({
                                        "type": "content_block_delta",
                                        "index": idx,
                                        "delta": {"type": "input_json_delta", "partial_json": args}
                                    }),
                                ));
                            }
                        }
                    }
                }
            }
        }

        // 检测 finish_reason 并映射为 Anthropic 停止原因
        if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
            self.finished = true;
            self.stop_reason = match fr {
                "stop" => "end_turn",       // 正常结束
                "tool_calls" => "tool_use", // 工具调用结束
                "length" => "max_tokens",   // 达到最大 token
                other => other,             // 其他原因原样透传
            }
            .to_string();
        }

        out
    }

    /// 流结束时发送收尾事件（content_block_stop / message_delta / message_stop）。
    ///
    /// 若流从未收到任何内容（未 started），则发送明确的错误事件，
    /// 避免客户端误判为"正常完成但无内容"。若已发生错误则返回空字符串。
    ///
    /// - 返回值：收尾 Anthropic SSE 事件字符串
    fn finish(&mut self) -> String {
        if self.errored {
            return String::new(); // 已出错，不再发送收尾
        }
        if !self.started {
            // 空流：发明确错误，避免客户端误判为"正常完成但无内容"
            return anthropic_error_event(
                "上游返回了空响应（无内容且未正常结束），可能被限流或服务异常，请重试",
            );
        }
        let mut out = String::new();
        // 关闭当前打开的内容块
        if let Some(ob) = self.open_block.take() {
            out.push_str(&sse_event(
                "content_block_stop",
                json!({"type": "content_block_stop", "index": ob}),
            ));
        }
        // 发送 message_delta（含停止原因与 token 用量）
        out.push_str(&sse_event(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {"stop_reason": self.stop_reason, "stop_sequence": Value::Null},
                "usage": {"input_tokens": self.input_tokens, "output_tokens": self.output_tokens}
            }),
        ));
        // 发送 message_stop 结束事件
        out.push_str(&sse_event("message_stop", json!({"type": "message_stop"})));
        out
    }
}
