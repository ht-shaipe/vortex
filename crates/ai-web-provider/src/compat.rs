//! OpenAI / Anthropic 协议兼容层。
//!
//! 在既有平台 worker（doubao/deepseek/qianwen/kimi）之上提供标准协议端点，
//! 让 OpenAI / Anthropic 生态客户端（SDK、Claude Code、Cherry Studio 等）
//! 直接接入：
//!
//! - `GET  /v1/models`            — 模型列表（同时满足 OpenAI/Anthropic 形状）
//! - `POST /v1/chat/completions`  — OpenAI Chat Completions（含流式 SSE）
//! - `POST /v1/messages`          — Anthropic Messages（含流式 SSE 事件序列）
//!
//! 约定：
//! - `model` 字段即平台名：`"doubao"` / `"kimi-k2"` / `"qianwen-max"` 等，
//!   精确命中或 `平台名 + 分隔符(-_.:/)` 前缀命中均可。
//! - 会话上下文来自浏览器聊天页（同一 worker 内多轮自动延续），
//!   每次请求只下发 system 前缀 + 最后一条用户消息，不回放历史，
//!   避免往浏览器会话里重复追加整段对话。
//! - 思考过程：非流式 OpenAI 响应放 `message.reasoning_content`（DeepSeek 惯例）；
//!   流式 OpenAI chunk 放 `delta.reasoning_content`；Anthropic 走独立
//!   `thinking` 内容块（content_block_start/delta）。
//! - 不支持 tools/function calling（网关是浏览器自动化，无工具通道），
//!   请求中的 tools 字段被忽略。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse};
use bytes::Bytes;
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::common::chunk::{decode_chunk, ChunkKind};
use crate::models::{AiReply, Command};
use crate::server::AppState;

// =============================================================================
// 公共小工具
// =============================================================================

/// 响应 id / created 时间戳（无 uuid/chrono 依赖，时间戳+计数器足够唯一）
struct Ids {
    plain: String,
    created: u64,
}

fn new_ids() -> Ids {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ids {
        plain: format!("{created:x}{n:x}"),
        created,
    }
}

/// 提取消息 content 中的纯文本：兼容字符串与内容块数组
/// （OpenAI content parts / Anthropic content blocks 的 `{"type":"text",...}`）。
fn content_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| {
                if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                    p.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// model 字段 → 平台名。精确命中优先，其次「平台名 + 分隔符前缀」
/// （doubao-seed / kimi-k2 / qianwen-max / deepseek-chat）。
/// 大小写不敏感（"Doubao" / "KIMI-K2" 均可）。
fn resolve_model(state: &AppState, model: &str) -> Option<String> {
    let model = model.trim().to_lowercase();
    if model.is_empty() {
        return None;
    }
    if state.workers.contains_key(model.as_str()) {
        return Some(model);
    }
    // 长平台名优先，避免前缀遮蔽
    let mut names: Vec<&String> = state.workers.keys().collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        if let Some(rest) = model.strip_prefix(name.as_str()) {
            if rest.is_empty()
                || matches!(rest.as_bytes()[0], b'-' | b'_' | b':' | b'.' | b'/')
            {
                return Some(name.clone());
            }
        }
    }
    None
}

/// 发起一次 ask（非流式），返回完整回复。
async fn ask_blocking(
    state: &AppState,
    platform: &str,
    question: String,
) -> Result<AiReply, String> {
    let tx = state
        .workers
        .get(platform)
        .cloned()
        .ok_or_else(|| "worker 已退出".to_string())?;
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(Command::Ask {
            question,
            chunk_tx: None,
            reply_tx,
        })
        .await
        .is_err()
    {
        return Err("worker 已退出".into());
    }
    match reply_rx.await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("worker 已退出".into()),
    }
}

/// 发起一次 ask（流式），返回 (增量通道, 最终结果通道)。
async fn ask_streaming(
    state: &AppState,
    platform: &str,
    question: String,
) -> Result<(mpsc::Receiver<String>, oneshot::Receiver<anyhow::Result<AiReply>>), String> {
    let tx = state
        .workers
        .get(platform)
        .cloned()
        .ok_or_else(|| "worker 已退出".to_string())?;
    let (chunk_tx, chunk_rx) = mpsc::channel(64);
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(Command::Ask {
            question,
            chunk_tx: Some(chunk_tx),
            reply_tx,
        })
        .await
        .is_err()
    {
        return Err("worker 已退出".into());
    }
    Ok((chunk_rx, reply_rx))
}

/// SSE 响应（text/event-stream，通道关闭即流结束）。
/// X-Accel-Buffering: no —— 关闭 nginx 等反代的响应缓冲，保证事件实时下发。
fn sse_response(rx: mpsc::Receiver<Bytes>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(ReceiverStream::new(rx).map(Ok::<Bytes, actix_web::Error>))
}

// =============================================================================
// GET /v1/models
// =============================================================================

/// 模型列表。每个条目同时带 OpenAI（object/owned_by）与
/// Anthropic（type/display_name）字段，两套 SDK 都能解析。
pub async fn list_models(state: web::Data<AppState>) -> HttpResponse {
    let mut names: Vec<&String> = state.workers.keys().collect();
    names.sort();
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let data: Vec<Value> = names
        .iter()
        .map(|n| {
            json!({
                "id": n,
                "object": "model",
                "type": "model",
                "created": created,
                "owned_by": "ai-web-gateway",
                "display_name": n,
            })
        })
        .collect();
    HttpResponse::Ok().json(json!({ "object": "list", "data": data }))
}

// =============================================================================
// OpenAI 兼容：POST /v1/chat/completions
// =============================================================================

/// OpenAI 风格错误响应：`{"error":{"message":..,"type":..}}`
fn openai_error(status: StatusCode, kind: &str, msg: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({
        "error": {"message": msg, "type": kind, "param": null, "code": null}
    }))
}

/// 从 OpenAI 请求体提取要下发的问题：system（含 developer）拼接 +
/// 最后一条 user 消息。assistant/tool 历史不回放（浏览器会话自带上下文）。
fn openai_question(body: &Value) -> Option<String> {
    let msgs = body.get("messages")?.as_array()?;
    let mut system: Vec<String> = Vec::new();
    let mut last_user = String::new();
    for m in msgs {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let text = content_text(m.get("content"));
        if text.is_empty() {
            continue;
        }
        match role {
            "system" | "developer" => system.push(text),
            "user" => last_user = text,
            _ => {}
        }
    }
    let sys = system.join("\n\n");
    let q = if sys.is_empty() {
        last_user
    } else if last_user.is_empty() {
        sys
    } else {
        format!("{sys}\n\n{last_user}")
    };
    if q.trim().is_empty() {
        None
    } else {
        Some(q)
    }
}

/// OpenAI 非流式响应：chat.completion（思考放 message.reasoning_content）
fn openai_completion(ids: &Ids, model: &str, reply: &AiReply) -> Value {
    let mut message = json!({"role": "assistant", "content": reply.reply});
    if !reply.thinking.is_empty() {
        message["reasoning_content"] = json!(reply.thinking);
    }
    json!({
        "id": format!("chatcmpl-{}", ids.plain),
        "object": "chat.completion",
        "created": ids.created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
    })
}

/// `POST /v1/chat/completions`：解析 OpenAI 请求体，转发到对应平台 worker。
///
/// `stream=true` 时返回 SSE（`chat.completion.chunk`，以 `[DONE]` 收尾）；
/// 否则等待完整回复后返回单个 `chat.completion`。
pub async fn chat_completions(state: web::Data<AppState>, body: web::Json<Value>) -> HttpResponse {
    let body = body.into_inner();
    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("");
    let Some(platform) = resolve_model(&state, model) else {
        return openai_error(
            StatusCode::NOT_FOUND,
            "invalid_request_error",
            &format!(
                "未知模型: {model:?}（可用平台: {}，model 填平台名或 平台名-xxx）",
                state.enabled_list()
            ),
        );
    };
    let Some(question) = openai_question(&body) else {
        return openai_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "messages 中没有可用的用户问题文本",
        );
    };
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let model = model.to_string();

    if !stream {
        return match ask_blocking(&state, &platform, question).await {
            Ok(reply) => {
                let ids = new_ids();
                HttpResponse::Ok().json(openai_completion(&ids, &model, &reply))
            }
            Err(e) => openai_error(StatusCode::BAD_GATEWAY, "api_error", &e),
        };
    }

    let (chunk_rx, reply_rx) = match ask_streaming(&state, &platform, question).await {
        Ok(pair) => pair,
        Err(e) => return openai_error(StatusCode::BAD_GATEWAY, "api_error", &e),
    };
    let (sse_tx, sse_rx) = mpsc::channel::<Bytes>(64);
    actix_web::rt::spawn(openai_stream_task(model, chunk_rx, reply_rx, sse_tx));
    sse_response(sse_rx)
}

/// OpenAI 流式任务：增量通道 → chat.completion.chunk SSE，结束补 [DONE]。
async fn openai_stream_task(
    model: String,
    mut chunk_rx: mpsc::Receiver<String>,
    reply_rx: oneshot::Receiver<anyhow::Result<AiReply>>,
    tx: mpsc::Sender<Bytes>,
) {
    let ids = new_ids();
    let mk = |delta: Value, finish: Option<&str>| -> Bytes {
        let mut choice = json!({"index": 0, "delta": delta});
        choice["finish_reason"] = match finish {
            Some(f) => json!(f),
            None => Value::Null,
        };
        let chunk = json!({
            "id": format!("chatcmpl-{}", ids.plain),
            "object": "chat.completion.chunk",
            "created": ids.created,
            "model": model,
            "choices": [choice],
        });
        Bytes::from(format!("data: {chunk}\n\n"))
    };

    // 首 chunk：角色声明
    let _ = tx.send(mk(json!({"role": "assistant"}), None)).await;

    // 空闲心跳：SSE 注释行（: ping），客户端/SDK 按 SSE 规范忽略，
    // 防止长思考期间代理/客户端空闲超时断连。先吃掉 interval 的立即首 tick
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    ping.tick().await;
    loop {
        let chunk = tokio::select! {
            chunk = chunk_rx.recv() => match chunk {
                Some(c) => c,
                None => break,
            },
            _ = ping.tick() => {
                let _ = tx.send(Bytes::from(": ping\n\n")).await;
                continue;
            }
        };
        let delta = match decode_chunk(&chunk) {
            Some((ChunkKind::Thinking, t)) if !t.is_empty() => json!({"reasoning_content": t}),
            Some((ChunkKind::Text, t)) if !t.is_empty() => json!({"content": t}),
            // 未编码的纯文本 chunk（旧实现）按正文处理
            None if !chunk.is_empty() => json!({"content": chunk}),
            _ => continue,
        };
        let _ = tx.send(mk(delta, None)).await;
    }

    match reply_rx.await {
        Ok(Ok(_)) => {
            let _ = tx.send(mk(json!({}), Some("stop"))).await;
        }
        Ok(Err(e)) => {
            let _ = tx
                .send(Bytes::from(format!(
                    "data: {}\n\n",
                    json!({"error": {"message": e.to_string(), "type": "api_error"}})
                )))
                .await;
        }
        Err(_) => {
            let _ = tx
                .send(Bytes::from(format!(
                    "data: {}\n\n",
                    json!({"error": {"message": "worker 已退出", "type": "api_error"}})
                )))
                .await;
        }
    }
    let _ = tx.send(Bytes::from("data: [DONE]\n\n")).await;
}

// =============================================================================
// Anthropic 兼容：POST /v1/messages
// =============================================================================

/// Anthropic 风格错误响应：`{"type":"error","error":{"type":..,"message":..}}`
fn anthropic_error(status: StatusCode, kind: &str, msg: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({
        "type": "error",
        "error": {"type": kind, "message": msg}
    }))
}

/// 从 Anthropic 请求体提取要下发的问题：顶层 system + 最后一条 user 消息。
fn anthropic_question(body: &Value) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(sys) = body.get("system") {
        let t = content_text(Some(sys));
        if !t.is_empty() {
            parts.push(t);
        }
    }
    if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
        for m in msgs.iter().rev() {
            if m.get("role").and_then(|r| r.as_str()) == Some("user") {
                let t = content_text(m.get("content"));
                if !t.is_empty() {
                    parts.push(t);
                }
                break;
            }
        }
    }
    let q = parts.join("\n\n");
    if q.trim().is_empty() {
        None
    } else {
        Some(q)
    }
}

/// Anthropic 非流式响应：message（思考为独立 thinking 内容块）
fn anthropic_message(ids: &Ids, model: &str, reply: &AiReply) -> Value {
    let mut blocks: Vec<Value> = Vec::new();
    if !reply.thinking.is_empty() {
        blocks.push(json!({"type": "thinking", "thinking": reply.thinking}));
    }
    // 至少一个内容块；reply 为空且无思考时给空 text 块
    if !reply.reply.is_empty() || blocks.is_empty() {
        blocks.push(json!({"type": "text", "text": reply.reply}));
    }
    json!({
        "id": format!("msg_{}", ids.plain),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": blocks,
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 0, "output_tokens": 0}
    })
}

/// `POST /v1/messages`：解析 Anthropic Messages 请求体，转发到对应平台 worker。
///
/// `stream=true` 时返回 Anthropic SSE 事件序列；否则返回单个 `message`。
pub async fn messages(state: web::Data<AppState>, body: web::Json<Value>) -> HttpResponse {
    let body = body.into_inner();
    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("");
    let Some(platform) = resolve_model(&state, model) else {
        return anthropic_error(
            StatusCode::NOT_FOUND,
            "not_found_error",
            &format!(
                "model 不存在: {model:?}（可用平台: {}，model 填平台名或 平台名-xxx）",
                state.enabled_list()
            ),
        );
    };
    let Some(question) = anthropic_question(&body) else {
        return anthropic_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "messages 中没有可用的用户问题文本",
        );
    };
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let model = model.to_string();

    if !stream {
        return match ask_blocking(&state, &platform, question).await {
            Ok(reply) => {
                let ids = new_ids();
                HttpResponse::Ok().json(anthropic_message(&ids, &model, &reply))
            }
            Err(e) => anthropic_error(StatusCode::BAD_GATEWAY, "api_error", &e),
        };
    }

    let (chunk_rx, reply_rx) = match ask_streaming(&state, &platform, question).await {
        Ok(pair) => pair,
        Err(e) => return anthropic_error(StatusCode::BAD_GATEWAY, "api_error", &e),
    };
    let (sse_tx, sse_rx) = mpsc::channel::<Bytes>(64);
    actix_web::rt::spawn(anthropic_stream_task(model, chunk_rx, reply_rx, sse_tx));
    sse_response(sse_rx)
}

/// 构造一条 Anthropic SSE 事件
fn anthropic_sse(name: &str, data: Value) -> Bytes {
    Bytes::from(format!("event: {name}\ndata: {data}\n\n"))
}

/// 某类内容块的 start + 首个 delta（用于流末尾补发完整回复）
fn anthropic_block_events(kind: &str, idx: usize, text: &str) -> Vec<Bytes> {
    let block = if kind == "thinking" {
        json!({"type": "thinking", "thinking": ""})
    } else {
        json!({"type": "text", "text": ""})
    };
    let delta = if kind == "thinking" {
        json!({"type": "thinking_delta", "thinking": text})
    } else {
        json!({"type": "text_delta", "text": text})
    };
    vec![
        anthropic_sse(
            "content_block_start",
            json!({"type": "content_block_start", "index": idx, "content_block": block}),
        ),
        anthropic_sse(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": idx, "delta": delta}),
        ),
    ]
}

/// Anthropic 流式任务：增量通道 → Messages SSE 事件序列
/// （message_start → ping → content_block_* → message_delta → message_stop）。
async fn anthropic_stream_task(
    model: String,
    mut chunk_rx: mpsc::Receiver<String>,
    reply_rx: oneshot::Receiver<anyhow::Result<AiReply>>,
    tx: mpsc::Sender<Bytes>,
) {
    let ids = new_ids();
    // message_start + ping
    let _ = tx
        .send(anthropic_sse(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": format!("msg_{}", ids.plain),
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            }),
        ))
        .await;
    let _ = tx
        .send(anthropic_sse("ping", json!({"type": "ping"})))
        .await;

    let mut next_index: usize = 0;
    // 当前打开的内容块 (索引, "thinking"|"text")
    let mut open: Option<(usize, &'static str)> = None;
    let mut saw_any = false;

    // 空闲心跳：Anthropic 标准 ping 事件（SDK 会忽略），防止长思考期间
    // 代理/客户端空闲超时断连。先吃掉 interval 的立即首 tick
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    ping.tick().await;
    loop {
        let chunk = tokio::select! {
            chunk = chunk_rx.recv() => match chunk {
                Some(c) => c,
                None => break,
            },
            _ = ping.tick() => {
                let _ = tx
                    .send(anthropic_sse("ping", json!({"type": "ping"})))
                    .await;
                continue;
            }
        };
        let (kind, text) = match decode_chunk(&chunk) {
            Some((k, t)) => (k, t),
            // 未编码的纯文本 chunk（旧实现）按正文处理
            None => (ChunkKind::Text, chunk),
        };
        if text.is_empty() {
            continue;
        }
        let want: &'static str = if kind == ChunkKind::Thinking {
            "thinking"
        } else {
            "text"
        };
        // 内容块类型切换时先关旧块再开新块
        if open.as_ref().map(|(_, t)| *t) != Some(want) {
            if let Some((idx, _)) = open.take() {
                let _ = tx
                    .send(anthropic_sse(
                        "content_block_stop",
                        json!({"type": "content_block_stop", "index": idx}),
                    ))
                    .await;
            }
            let idx = next_index;
            next_index += 1;
            let block = if want == "thinking" {
                json!({"type": "thinking", "thinking": ""})
            } else {
                json!({"type": "text", "text": ""})
            };
            let _ = tx
                .send(anthropic_sse(
                    "content_block_start",
                    json!({"type": "content_block_start", "index": idx, "content_block": block}),
                ))
                .await;
            open = Some((idx, want));
        }
        let (idx, _) = open.expect("内容块已开启");
        let delta = if want == "thinking" {
            json!({"type": "thinking_delta", "thinking": text})
        } else {
            json!({"type": "text_delta", "text": text})
        };
        let _ = tx
            .send(anthropic_sse(
                "content_block_delta",
                json!({"type": "content_block_delta", "index": idx, "delta": delta}),
            ))
            .await;
        saw_any = true;
    }

    match reply_rx.await {
        Ok(Ok(reply)) => {
            // 流中无任何内容时从完整回复补发（正常路径不会走到，兜底）
            if !saw_any {
                let mut fallback_idx = next_index;
                if !reply.thinking.is_empty() {
                    for ev in anthropic_block_events("thinking", fallback_idx, &reply.thinking) {
                        let _ = tx.send(ev).await;
                    }
                    open = Some((fallback_idx, "thinking"));
                    fallback_idx += 1;
                }
                if !reply.reply.is_empty() {
                    for ev in anthropic_block_events("text", fallback_idx, &reply.reply) {
                        let _ = tx.send(ev).await;
                    }
                    open = Some((fallback_idx, "text"));
                }
            }
            if let Some((idx, _)) = open.take() {
                let _ = tx
                    .send(anthropic_sse(
                        "content_block_stop",
                        json!({"type": "content_block_stop", "index": idx}),
                    ))
                    .await;
            }
            let _ = tx
                .send(anthropic_sse(
                    "message_delta",
                    json!({
                        "type": "message_delta",
                        "delta": {"stop_reason": "end_turn", "stop_sequence": null},
                        "usage": {"input_tokens": 0, "output_tokens": 0}
                    }),
                ))
                .await;
            let _ = tx
                .send(anthropic_sse("message_stop", json!({"type": "message_stop"})))
                .await;
        }
        Ok(Err(e)) => {
            if let Some((idx, _)) = open.take() {
                let _ = tx
                    .send(anthropic_sse(
                        "content_block_stop",
                        json!({"type": "content_block_stop", "index": idx}),
                    ))
                    .await;
            }
            let _ = tx
                .send(anthropic_sse(
                    "error",
                    json!({"type": "error", "error": {"type": "api_error", "message": e.to_string()}}),
                ))
                .await;
        }
        Err(_) => {
            let _ = tx
                .send(anthropic_sse(
                    "error",
                    json!({"type": "error", "error": {"type": "api_error", "message": "worker 已退出"}}),
                ))
                .await;
        }
    }
}
