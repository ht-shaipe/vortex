//! POST /v1/chat/completions 聊天补全端点
//!
//! 该端点是 Vortex 的核心代理入口，兼容 OpenAI Chat Completions API。
//! 接收 OpenAI 格式请求体，经 [`parse_chat_request`] 解析为内部 [`ProxyRequest`]，
//! 再交由 [`proxy_engine`](crate::AppState::proxy_engine) 统一路由与转发。
//! 支持流式（SSE）与非流式两种响应模式。

use actix_web::{web, HttpRequest, HttpResponse};
use crate::db::models::ProxyRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 POST /v1/chat/completions 请求。
///
/// 解析请求体中的 `model`、`stream` 等字段以及 `Authorization` 头中的 API 密钥，
/// 构造 [`ProxyRequest`] 后交给代理引擎处理。根据引擎输出决定返回普通响应还是 SSE 流。
///
/// - `state`：应用全局状态（数据库连接池、代理引擎等）
/// - `body`：OpenAI 格式的 JSON 请求体
/// - `req`：原始 HTTP 请求（用于提取鉴权头）
/// - 返回值：`HttpResponse`，成功时为 JSON 响应或 SSE 流；失败时为 OpenAI 风格错误
pub async fn chat_completions(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
    req: HttpRequest,
) -> HttpResponse {
    let body_inner = body.into_inner(); // 取出请求体所有权
    // 解析请求体为内部 ProxyRequest，失败则返回 OpenAI 风格错误
    let request = match parse_chat_request(&body_inner, &req) {
        Ok(r) => r,
        Err(e) => return super::openai_error_response(&e),
    };

    // 获取代理引擎读锁，处理请求
    let engine = state.proxy_engine.read();
    match engine.handle_request(&state, request).await {
        // 非流式响应：直接返回
        Ok(crate::proxy::engine::ProxyOutput::Response(response)) => response,
        // 流式响应：包装为 SSE HTTP 响应
        Ok(crate::proxy::engine::ProxyOutput::Stream(stream)) => {
            crate::proxy::sse::sse_http_response(stream)
        }
        // 引擎错误：返回 OpenAI 风格错误
        Err(e) => super::openai_error_response(&e),
    }
}

/// 将 OpenAI 格式请求体解析为内部 [`ProxyRequest`]。
///
/// 从 JSON 中提取 `model`（必填）、`stream`（默认 false）、`temperature`、`max_tokens`、
/// `top_p` 等参数，并从 `Authorization: Bearer <key>` 头中提取 API 密钥。
///
/// - `body`：OpenAI 格式 JSON 请求体
/// - `req`：HTTP 请求，用于提取鉴权头
/// - 返回值：解析成功的 [`ProxyRequest`] 或 [`AppError`](crate::error::AppError)
fn parse_chat_request(body: &serde_json::Value, req: &HttpRequest) -> crate::error::Result<ProxyRequest> {
    // 提取模型名（必填字段）
    let model = body.get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("model is required".into()))?
        .to_string();

    // 提取流式标志，默认 false
    let stream = body.get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 从 Authorization Bearer 头提取 API 密钥
    let api_key = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // 构造内部代理请求
    Ok(ProxyRequest {
        model,
        messages: body.clone(), // 保留完整请求体作为消息
        stream,
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        max_tokens: body.get("max_tokens").and_then(|v| v.as_i64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        api_key,
        source_format: "openai".to_string(), // 标记来源格式为 OpenAI
        extra: json!({}),
    })
}
