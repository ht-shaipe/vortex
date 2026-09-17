//! POST /v1/responses OpenAI Responses API 兼容端点
//!
//! 该端点为 Codex 等客户端提供 OpenAI Responses API 兼容层。
//! Responses API 是 OpenAI 推出的新一代 API 格式，与 Chat Completions API 有所不同。
//! 本模块负责将 Responses 请求转换为内部 [`ProxyRequest`]，复用现有代理引擎，
//! 再将响应转换回 Responses API 格式。

use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use serde_json::json;
use std::sync::Arc;

use crate::db::models::ProxyRequest;
use crate::proxy::sse::SseStream;
use crate::AppState;

/// 处理 POST /v1/responses 请求。
///
/// 将 Responses API 格式请求转换为内部 ProxyRequest，复用代理引擎处理，
/// 再将响应转换回 Responses API 格式。
///
/// - `state`：应用全局状态
/// - `body`：Responses API 格式的 JSON 请求体
/// - `req`：原始 HTTP 请求（用于提取鉴权头）
/// - 返回值：`HttpResponse`，Responses API 格式的 JSON 或 SSE 流
pub async fn responses(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
    req: HttpRequest,
) -> HttpResponse {
    let body_inner = body.into_inner();

    // 解析 Responses API 请求
    let request = match parse_responses_request(&body_inner, &req) {
        Ok(r) => r,
        Err(e) => return super::openai_error_response(&e),
    };

    // 引擎无内部可变性，直接借用处理请求（避免锁守卫跨 await）
    let upstream_client = crate::create_awc_client();

    match state
        .proxy_engine
        .handle_request(&state, &upstream_client, request)
        .await
    {
        Ok(crate::proxy::engine::ProxyOutput::Response(body)) => {
            // 非流式：转换为 Responses API 格式
            match convert_to_responses_format(&body, &body_inner) {
                Ok(response) => HttpResponse::Ok().json(response),
                Err(e) => super::openai_error_response(&e),
            }
        }
        Ok(crate::proxy::engine::ProxyOutput::Stream(stream)) => {
            // 流式：转换为 Responses API SSE 格式
            convert_to_responses_stream(stream, &body_inner)
        }
        Err(e) => super::openai_error_response(&e),
    }
}

/// 解析 Responses API 请求为内部 ProxyRequest。
///
/// Responses API 使用 `input` 字段而非 `messages`，支持多种输入格式：
/// - 字符串：简单文本输入
/// - 消息数组：包含 role/content 的结构化输入
/// - 工具调用结果：function_call_output 等
fn parse_responses_request(
    body: &serde_json::Value,
    req: &HttpRequest,
) -> crate::error::Result<ProxyRequest> {
    // 提取模型名（必填）
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("model is required".into()))?
        .to_string();

    // 提取流式标志
    let stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 提取 API 密钥
    let api_key = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // 将 Responses input 转换为 Chat messages 格式
    let messages = convert_input_to_messages(body)?;

    // 构造 Chat Completions 格式的请求体
    let chat_body = build_chat_request(body, &messages)?;

    Ok(ProxyRequest {
        model,
        messages: chat_body,
        stream,
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        max_tokens: body.get("max_output_tokens").and_then(|v| v.as_i64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        api_key,
        agent: super::detect_agent(req.headers()),
        source_format: "responses".to_string(),
        extra: json!({
            "original_format": "responses",
            "tools": body.get("tools").cloned().unwrap_or(json!([])),
            "tool_choice": body.get("tool_choice").cloned().unwrap_or(json!("auto")),
        }),
        saved_tokens: 0,
    })
}

/// 将 Responses API 的 input 转换为 Chat messages 格式。
///
/// Responses API 的 input 可以是：
/// - 字符串：转换为单条 user 消息
/// - 数组：包含多种类型的输入项
fn convert_input_to_messages(body: &serde_json::Value) -> crate::error::Result<serde_json::Value> {
    let input = body.get("input").ok_or_else(|| {
        crate::error::AppError::BadRequest("input is required".into())
    })?;

    let messages = if let Some(text) = input.as_str() {
        // 字符串输入：转换为单条 user 消息
        json!([
            {
                "role": "user",
                "content": text
            }
        ])
    } else if let Some(items) = input.as_array() {
        // 数组输入：逐项转换
        let mut msgs = Vec::new();
        for item in items {
            if let Some(msg) = convert_input_item(item)? {
                msgs.push(msg);
            }
        }
        json!(msgs)
    } else {
        return Err(crate::error::AppError::BadRequest(
            "input must be a string or array".into(),
        ));
    };

    Ok(messages)
}

/// 转换单个 input 项为 message 格式。
///
/// 支持的 input 类型：
/// - message：直接转换为消息
/// - function_call_output：转换为 tool 角色消息
/// - 其他类型：暂不支持
fn convert_input_item(item: &serde_json::Value) -> crate::error::Result<Option<serde_json::Value>> {
    let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match item_type {
        "message" => {
            // 消息类型：提取 role 和 content
            let role = item
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("user");
            let content = item.get("content").ok_or_else(|| {
                crate::error::AppError::BadRequest("message content is required".into())
            })?;

            // content 可以是字符串或数组
            let content_value = if let Some(text) = content.as_str() {
                json!(text)
            } else if let Some(parts) = content.as_array() {
                // 内容数组：提取文本部分
                let texts: Vec<_> = parts
                    .iter()
                    .filter_map(|p| {
                        if p.get("type").and_then(|t| t.as_str()) == Some("input_text") {
                            p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                json!(texts.join("\n"))
            } else {
                content.clone()
            };

            Ok(Some(json!({
                "role": role,
                "content": content_value
            })))
        }
        "function_call_output" => {
            // 工具调用结果：转换为 tool 角色消息
            let call_id = item.get("call_id").and_then(|c| c.as_str()).ok_or_else(|| {
                crate::error::AppError::BadRequest("function_call_output requires call_id".into())
            })?;
            let output = item.get("output").and_then(|o| o.as_str()).unwrap_or("");

            Ok(Some(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            })))
        }
        _ => {
            // 其他类型暂不支持，跳过
            Ok(None)
        }
    }
}

/// 构建 Chat Completions 格式的请求体。
///
/// 将 Responses API 的参数转换为 Chat Completions 格式，
/// 保留 tools、tool_choice 等字段。
fn build_chat_request(
    original: &serde_json::Value,
    messages: &serde_json::Value,
) -> crate::error::Result<serde_json::Value> {
    let mut chat_body = json!({
        "model": original.get("model").cloned().unwrap_or(json!("")),
        "messages": messages,
        "stream": original.get("stream").cloned().unwrap_or(json!(false)),
    });

    // 复制可选参数
    if let Some(temp) = original.get("temperature") {
        chat_body["temperature"] = temp.clone();
    }
    if let Some(top_p) = original.get("top_p") {
        chat_body["top_p"] = top_p.clone();
    }
    if let Some(max_tokens) = original.get("max_output_tokens") {
        chat_body["max_tokens"] = max_tokens.clone();
    }

    // 复制工具相关字段
    if let Some(tools) = original.get("tools") {
        chat_body["tools"] = tools.clone();
    }
    if let Some(tool_choice) = original.get("tool_choice") {
        chat_body["tool_choice"] = tool_choice.clone();
    }

    Ok(chat_body)
}

/// 将 Chat Completions 响应转换为 Responses API 格式。
///
/// Responses API 响应结构包含：
/// - id：响应 ID
/// - output：输出项数组
/// - status：完成状态
/// - 其他元数据
fn convert_to_responses_format(
    chat_response: &serde_json::Value,
    original_request: &serde_json::Value,
) -> crate::error::Result<serde_json::Value> {
    // 生成响应 ID
    let response_id = format!("resp_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));

    // 提取 choices
    let choices = chat_response
        .get("choices")
        .and_then(|c| c.as_array())
        .ok_or_else(|| {
            crate::error::AppError::Provider("Invalid chat response format".into())
        })?;

    // 转换 choices 为 output 项
    let mut output_items = Vec::new();
    for choice in choices {
        let message = choice.get("message").ok_or_else(|| {
            crate::error::AppError::Provider("Missing message in choice".into())
        })?;

        // 创建 message 输出项
        let content = message.get("content").and_then(|c| c.as_str()).unwrap_or("");
        let mut output_item = json!({
            "type": "message",
            "id": format!("msg_{}", uuid::Uuid::new_v4().to_string().replace("-", "")),
            "status": "completed",
            "role": "assistant",
            "content": [
                {
                    "type": "output_text",
                    "text": content
                }
            ]
        });

        // 处理工具调用
        if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
            let mut tool_outputs = Vec::new();
            for call in tool_calls {
                let call_id = call.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let function = call.get("function").ok_or_else(|| {
                    crate::error::AppError::Provider("Missing function in tool call".into())
                })?;
                let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let arguments = function.get("arguments").and_then(|a| a.as_str()).unwrap_or("");

                tool_outputs.push(json!({
                    "type": "function_call",
                    "id": format!("call_{}", uuid::Uuid::new_v4().to_string().replace("-", "")),
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments,
                    "status": "completed"
                }));
            }
            output_item["content"] = json!(tool_outputs);
        }

        output_items.push(output_item);
    }

    // 构造 Responses 响应
    let finish_reason = choices
        .first()
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .unwrap_or("stop");

    let status = match finish_reason {
        "stop" => "completed",
        "length" => "incomplete",
        "tool_calls" => "completed",
        _ => "completed",
    };

    Ok(json!({
        "id": response_id,
        "object": "response",
        "created_at": chat_response.get("created").cloned().unwrap_or(json!(0)),
        "model": chat_response.get("model").cloned().unwrap_or(json!("")),
        "output": output_items,
        "status": status,
        "usage": chat_response.get("usage").cloned().unwrap_or(json!({})),
        "temperature": chat_response.get("temperature").cloned().unwrap_or(json!(1.0)),
        "top_p": chat_response.get("top_p").cloned().unwrap_or(json!(1.0)),
        "max_output_tokens": original_request.get("max_output_tokens").cloned().unwrap_or(json!(0))
    }))
}

/// 将 Chat Completions 流式响应转换为 Responses API SSE 格式。
///
/// Responses API 流式事件包括：
/// - response.created：响应创建
/// - response.in_progress：处理中
/// - response.output_item.added：添加输出项
/// - response.content_part.added：添加内容部分
/// - response.output_text.delta：文本增量
/// - response.output_text.done：文本完成
/// - response.completed：响应完成
fn convert_to_responses_stream(
    stream: SseStream,
    _original_request: &serde_json::Value,
) -> HttpResponse {
    use bytes::Bytes;
    use std::pin::Pin;

    let response_id = format!("resp_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    let _created_at = chrono::Utc::now().timestamp();

    // 转换流并装箱
    let responses_stream: Pin<Box<dyn futures::Stream<Item = Result<Bytes, std::io::Error>> + 'static>> = 
        Box::pin(stream.map(move |chunk_result| {
            match chunk_result {
                Ok(bytes) => {
                    let chunk_str = String::from_utf8_lossy(&bytes);
                    // 解析 Chat Completions SSE 事件
                    for line in chunk_str.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                // 发送完成事件
                                return Ok::<_, std::io::Error>(Bytes::from(format!(
                                    "event: response.completed\ndata: {}\n\n",
                                    json!({
                                        "type": "response.completed",
                                        "response": {
                                            "id": response_id,
                                            "status": "completed",
                                            "output": []
                                        }
                                    })
                                )));
                            }

                            if let Ok(chat_chunk) = serde_json::from_str::<serde_json::Value>(data) {
                                // 转换为 Responses 事件
                                if let Some(event) = convert_chat_chunk_to_responses(
                                    &chat_chunk,
                                    &response_id,
                                ) {
                                    return Ok(Bytes::from(event));
                                }
                            }
                        }
                    }
                    Ok(Bytes::new())
                }
                Err(e) => Err(std::io::Error::other(e.to_string())),
            }
        }));

    crate::proxy::sse::sse_http_response(responses_stream)
}

/// 将 Chat Completions 流块转换为 Responses API 事件。
fn convert_chat_chunk_to_responses(
    chunk: &serde_json::Value,
    _response_id: &str,
) -> Option<String> {
    let choices = chunk.get("choices")?.as_array()?;
    let choice = choices.first()?;
    let delta = choice.get("delta")?;

    // 检查是否有内容增量
    if let Some(content) = delta.get("content").and_then(|c| c.as_str())
        && !content.is_empty() {
            return Some(format!(
                "event: response.output_text.delta\ndata: {}\n\n",
                json!({
                    "type": "response.output_text.delta",
                    "delta": content
                })
            ));
        }

    // 检查是否有工具调用增量
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
        for call in tool_calls {
            if let Some(function) = call.get("function")
                && let Some(arguments) = function.get("arguments").and_then(|a| a.as_str())
                    && !arguments.is_empty() {
                        return Some(format!(
                            "event: response.function_call_arguments.delta\ndata: {}\n\n",
                            json!({
                                "type": "response.function_call_arguments.delta",
                                "delta": arguments
                            })
                        ));
                    }
        }
    }

    None
}
