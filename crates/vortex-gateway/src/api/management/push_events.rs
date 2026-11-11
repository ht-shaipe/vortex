//! 中央推送事件回填端点
//!
//! - `GET /api/push-events`：返回最近推送的通知（内存缓冲，新→旧，至多 50 条）
//!   与当前连接状态 `connected`（hub 握手成功为 true）。
//!
//! 推送不经过轮询通道：webview 未挂载期间（应用启动瞬间）或窗口刷新后，
//! 前端靠本端点恢复收件箱内容；弹过即已读（localStorage 已读 ID 去重），
//! 重复拉取不会重复弹窗。`connected` 用于前端/运维确认通道连通性
//! （对齐 geo-pet 的 hello-ack 状态上报）。

use actix_web::{web, HttpResponse};
use std::sync::Arc;

use crate::AppState;
use crate::services::push_service;

/// 处理 `GET /api/push-events`，返回最近推送事件列表与连接状态。
pub async fn list_recent(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let events = push_service::recent_events(&state.push_history);
    let connected = state.push_connected.load(std::sync::atomic::Ordering::Relaxed);
    HttpResponse::Ok().json(serde_json::json!({ "events": events, "connected": connected }))
}
