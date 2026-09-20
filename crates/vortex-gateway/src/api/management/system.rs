//! 系统状态与代理控制端点。
//!
//! 提供 HTTP 端点供 Web 模式前端获取系统运行状态、启停本地代理服务器。
//! 业务逻辑委托给 services 层，同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池。

use actix_web::{web, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;
use crate::services::{proxy_service, system_service};

/// `GET /api/system/status` — 获取系统运行状态快照。
pub async fn get_system_status(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || system_service::get_system_status(&state)).await;
    match result {
        Ok(Ok(status)) => HttpResponse::Ok().json(status),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// `POST /api/system/proxy/start` — 启动本地代理服务器。
pub async fn start_proxy(state: web::Data<Arc<AppState>>) -> HttpResponse {
    match proxy_service::start_proxy(&state) {
        Ok(()) => HttpResponse::Ok().json(json!({ "status": "ok" })),
        Err(e) => HttpResponse::BadRequest().json(json!({ "error": e })),
    }
}

/// `POST /api/system/proxy/stop` — 停止本地代理服务器。
pub async fn stop_proxy(state: web::Data<Arc<AppState>>) -> HttpResponse {
    match proxy_service::stop_proxy(&state).await {
        Ok(()) => HttpResponse::Ok().json(json!({ "status": "ok" })),
        Err(e) => HttpResponse::BadRequest().json(json!({ "error": e })),
    }
}
