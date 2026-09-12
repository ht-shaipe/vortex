//! 聊天流式对话 Tauri IPC 命令。
//!
//! 桌面端对话不走 webview 的 fetch + SSE（WKWebView 对流式响应体支持不稳，
//! 且依赖本地端口可达），而是通过 Tauri IPC [`Channel`] 把上游 SSE 增量
//! 逐块推送给前端，前端按与 HTTP SSE 相同的 `data:` 行协议解析。
//!
//! - [`chat_completions_stream`]：执行一次对话请求，经代理引擎路由到上游，
//!   流式增量通过 `on_event` 通道实时下发；支持通过 [`cancel_chat_stream`] 取消。
//! - [`cancel_chat_stream`]：标记指定请求取消，流循环在下个增量前停止。

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use futures::StreamExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::proxy::engine::ProxyOutput;
use crate::AppState;

/// IPC 流式对话事件，序列化为 `{ "type": "...", "data": {...} }` 形式下发前端。
#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum ChatStreamEvent {
    /// 流式增量：上游归一化 SSE 的原始文本片段（与 HTTP 响应体字节一致）
    Delta {
        /// SSE 文本片段
        text: String,
    },
    /// 非流式完整响应：上游忽略 stream 参数时返回的完整 JSON 体
    Complete {
        /// OpenAI 格式完整响应 JSON 文本
        body: String,
    },
    /// 流正常结束
    Done { ok: bool },
    /// 处理失败（路由失败、上游错误、流中断等）
    Error {
        /// 可读错误信息
        message: String,
    },
}

/// 已取消的请求 ID 集合（跨命令共享）。
fn cancelled_set() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 标记请求已取消。
fn mark_cancelled(request_id: &str) {
    if !request_id.is_empty() {
        cancelled_set().lock().unwrap().insert(request_id.to_string());
    }
}

/// 消费取消标记（读取后清除，保证同 ID 只生效一次）。
fn take_cancelled(request_id: &str) -> bool {
    if request_id.is_empty() {
        return false;
    }
    cancelled_set().lock().unwrap().remove(request_id)
}

/// 执行流式对话并通过 IPC Channel 下发增量。
///
/// 与 HTTP `/v1/chat/completions` 共用同一代理引擎（路由、重试、熔断、用量记录完全一致），
/// 区别仅在传输层：响应不经过 actix，而是把归一化 SSE 流逐块发送到前端 `on_event` 通道。
///
/// # 参数
/// - `state`：应用全局状态
/// - `on_event`：前端事件通道
/// - `request_id`：请求标识，配合 [`cancel_chat_stream`] 实现取消
/// - `model`：目标模型（完整 id，如 `custom-openai/deepseek-ai/...`）
/// - `messages`：OpenAI 格式消息数组
/// - `stream`：是否流式（默认 true）
/// - `temperature` / `max_tokens`：可选采样参数
#[tauri::command]
pub async fn chat_completions_stream(
    state: State<'_, std::sync::Arc<AppState>>,
    on_event: Channel<ChatStreamEvent>,
    request_id: String,
    model: String,
    messages: serde_json::Value,
    stream: Option<bool>,
    temperature: Option<f64>,
    max_tokens: Option<i64>,
) -> Result<(), String> {
    let app_state = state.inner().clone();
    // 把引擎克隆出读锁（守卫非 Send，不能跨 await 持有）
    let engine = app_state.proxy_engine.read().clone();

    // ProxyRequest.messages 约定携带完整 OpenAI 请求体对象（内含 messages 数组），
    // executor 在其上注入 model/stream 等字段。兼容两种传参：
    // 裸 messages 数组 → 包装为对象；完整请求体对象 → 原样使用。
    let body = match &messages {
        serde_json::Value::Array(_) => serde_json::json!({ "messages": messages }),
        serde_json::Value::Object(_) => messages,
        _ => {
            let _ = on_event.send(ChatStreamEvent::Error {
                message: "messages 参数须为消息数组或完整请求体对象".to_string(),
            });
            return Ok(());
        }
    };

    let request = crate::db::models::ProxyRequest {
        model,
        messages: body,
        stream: stream.unwrap_or(true),
        temperature,
        max_tokens,
        top_p: None,
        // 桌面 IPC 直连引擎，不经网关鉴权
        api_key: None,
        source_format: "openai".to_string(),
        extra: serde_json::Value::Null,
    };

    // 引擎内部把 rusqlite Connection 跨 await 持有（非 Send），
    // 必须在阻塞线程中执行。awc::Client 需要 actix runtime 驱动，
    // 因此在 spawn_blocking 线程中创建 actix_rt::Runtime 并 block_on。
    tauri::async_runtime::spawn_blocking(move || {
        let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
        rt.block_on(async {
                let upstream_client = crate::create_upstream_client(&app_state.upstream_ssl_connector);
                let output = engine.handle_request(&app_state, &upstream_client, request).await;
                match output {
                    Ok(ProxyOutput::Stream(mut sse)) => {
                        while let Some(chunk) = sse.next().await {
                            if take_cancelled(&request_id) {
                                let _ = on_event.send(ChatStreamEvent::Done { ok: true });
                                return;
                            }
                            match chunk {
                                Ok(bytes) => {
                                    let text = String::from_utf8_lossy(&bytes).into_owned();
                                    let _ = on_event.send(ChatStreamEvent::Delta { text });
                                }
                                Err(e) => {
                                    let _ = on_event.send(ChatStreamEvent::Error {
                                        message: e.to_string(),
                                    });
                                    return;
                                }
                            }
                        }
                        let _ = on_event.send(ChatStreamEvent::Done { ok: true });
                    }
                    Ok(ProxyOutput::Response(body)) => {
                        let _ = on_event.send(ChatStreamEvent::Complete {
                            body: body.to_string(),
                        });
                        let _ = on_event.send(ChatStreamEvent::Done { ok: true });
                    }
                    Err(e) => {
                        let _ = on_event.send(ChatStreamEvent::Error {
                            message: e.to_string(),
                        });
                    }
                }
            })
    })
    .await
    .map_err(|e| format!("IPC chat task failed: {}", e))?;

    Ok(())
}

/// 取消一次进行中的 IPC 流式对话。
///
/// 取消是协作式的：流循环在发送下一个增量前检查标记，
/// 因此上游已发出、尚在管道中的内容可能仍会到达。
///
/// # 参数
/// - `request_id`：[`chat_completions_stream`] 传入的请求标识
#[tauri::command]
pub fn cancel_chat_stream(request_id: String) {
    mark_cancelled(&request_id);
}
