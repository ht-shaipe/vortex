//! 对话控制端点。
//!
//! 提供 HTTP 端点供 Web 模式前端取消进行中的流式对话。
//! 流式对话本身复用 `POST /v1/chat/completions`（SSE），本模块仅提供取消功能。

use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::services::chat_service;

/// 取消请求体。
#[derive(Deserialize)]
pub struct CancelRequest {
    pub request_id: String,
}

/// `POST /api/chat/cancel` — 取消一次进行中的流式对话。
pub async fn cancel_chat_stream(body: web::Json<CancelRequest>) -> HttpResponse {
    chat_service::mark_cancelled(&body.request_id);
    HttpResponse::Ok().json(json!({ "status": "ok" }))
}
