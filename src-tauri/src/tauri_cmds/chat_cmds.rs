//! 聊天流式对话 Tauri IPC 命令。
//!
//! 桌面端对话不走 webview 的 fetch + SSE（WKWebView 对流式响应体支持不稳，
//! 且依赖本地端口可达），而是通过 Tauri IPC [`Channel`] 把上游 SSE 增量
//! 逐块推送给前端。
//!
//! 取消逻辑委托给 [`vortex_gateway::services::chat_service`]，
//! 流式传输（Channel）为本模块特有逻辑。

use std::sync::Arc;

use futures::StreamExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use vortex_router::proxy::engine::ProxyOutput;
use vortex_gateway::AppState;
use vortex_gateway::services::chat_service;

/// IPC 流式对话事件。
#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum ChatStreamEvent {
    Delta { text: String },
    Complete { body: String },
    Done { ok: bool },
    Error { message: String },
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn chat_completions_stream(
    state: State<'_, Arc<AppState>>,
    on_event: Channel<ChatStreamEvent>,
    request_id: String,
    model: String,
    messages: serde_json::Value,
    stream: Option<bool>,
    temperature: Option<f64>,
    max_tokens: Option<i64>,
) -> Result<(), String> {
    let app_state = state.inner().clone();
    let engine = app_state.proxy_engine.clone();

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

    let request = vortex_store::db::models::ProxyRequest {
        model,
        messages: body,
        stream: stream.unwrap_or(true),
        temperature,
        max_tokens,
        top_p: None,
        api_key: None,
        agent: Some("vortex_chat".to_string()),
        source_format: "openai".to_string(),
        extra: serde_json::Value::Null,
        saved_tokens: 0,
    };

    tauri::async_runtime::spawn_blocking(move || {
        let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
        rt.block_on(async {
            let upstream_client = vortex_gateway::create_awc_client();
            let output = engine.handle_request(&*app_state, &upstream_client, request).await;
            match output {
                Ok(ProxyOutput::Stream(mut sse)) => {
                    while let Some(chunk) = sse.next().await {
                        if chat_service::take_cancelled(&request_id) {
                            let _ = on_event.send(ChatStreamEvent::Done { ok: true });
                            return;
                        }
                        match chunk {
                            Ok(bytes) => {
                                let text = String::from_utf8_lossy(&bytes).into_owned();
                                let _ = on_event.send(ChatStreamEvent::Delta { text });
                            }
                            Err(e) => {
                                let _ = on_event.send(ChatStreamEvent::Error { message: e.to_string() });
                                return;
                            }
                        }
                    }
                    let _ = on_event.send(ChatStreamEvent::Done { ok: true });
                }
                Ok(ProxyOutput::Response(body)) => {
                    let _ = on_event.send(ChatStreamEvent::Complete { body: body.to_string() });
                    let _ = on_event.send(ChatStreamEvent::Done { ok: true });
                }
                Err(e) => {
                    let _ = on_event.send(ChatStreamEvent::Error { message: e.to_string() });
                }
            }
        })
    })
    .await
    .map_err(|e| format!("IPC chat task failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn cancel_chat_stream(request_id: String) {
    chat_service::mark_cancelled(&request_id);
}
