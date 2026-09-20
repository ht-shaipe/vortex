//! 智能体集成端点。
//!
//! 提供 HTTP 端点供 Web 模式前端检测智能体、预览/应用/恢复配置、列出备份。
//! 业务逻辑委托给 services 层，同步文件/DB 操作通过 `spawn_blocking` 移至阻塞线程池。

use actix_web::{web, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;
use crate::agent_integrations::AgentKind;
use crate::services::agent_service;

/// `GET /api/agents/detect` — 检测所有智能体。
pub async fn detect_agents() -> HttpResponse {
    let result = tokio::task::spawn_blocking(agent_service::detect_agents).await;
    match result {
        Ok(Ok(resp)) => HttpResponse::Ok().json(resp),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// `POST /api/agents/preview` — 预览配置变更。
pub async fn preview_config(
    state: web::Data<Arc<AppState>>,
    body: web::Json<AgentKind>,
) -> HttpResponse {
    let state = state.clone();
    let agent = body.into_inner();
    let result = tokio::task::spawn_blocking(move || agent_service::preview_config(&state, agent)).await;
    match result {
        Ok(Ok(resp)) => HttpResponse::Ok().json(resp),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// `POST /api/agents/apply` — 应用配置。
pub async fn apply_config(
    state: web::Data<Arc<AppState>>,
    body: web::Json<agent_service::ApplyConfigRequest>,
) -> HttpResponse {
    let state = state.clone();
    let request = body.into_inner();
    let result = tokio::task::spawn_blocking(move || agent_service::apply_config(&state, request)).await;
    match result {
        Ok(Ok(resp)) => HttpResponse::Ok().json(resp),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// `POST /api/agents/restore` — 恢复配置。
pub async fn restore_config(
    body: web::Json<agent_service::RestoreConfigRequest>,
) -> HttpResponse {
    let request = body.into_inner();
    let result = tokio::task::spawn_blocking(move || agent_service::restore_config(request)).await;
    match result {
        Ok(Ok(resp)) => HttpResponse::Ok().json(resp),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// `GET /api/agents/backups` — 列出备份。
pub async fn list_backups() -> HttpResponse {
    let result = tokio::task::spawn_blocking(agent_service::list_backups).await;
    match result {
        Ok(Ok(resp)) => HttpResponse::Ok().json(resp),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
