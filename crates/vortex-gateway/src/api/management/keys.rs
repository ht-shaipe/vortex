//! API 密钥管理端点
//!
//! 端点：
//! - `GET    /api/keys`：列出密钥（仅返回脱敏后的元数据）
//! - `POST   /api/keys`：创建密钥
//! - `GET    /api/keys/{id}`：按 ID 查询密钥
//! - `DELETE /api/keys/{id}`：删除密钥
//!
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use vortex_store::db::{core as db_core, api_keys as db_keys};
use vortex_store::db::models::CreateApiKeyRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 `GET /api/keys`，列出全部 API 密钥。
pub async fn list_keys(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_keys::list(&conn).map(|keys| json!({ "keys": keys })).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `POST /api/keys`，创建新密钥并返回明文 key（仅本次响应可见）。
pub async fn create_key(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateApiKeyRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<vortex_store::db::models::ApiKey, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_keys::create(&conn, &req).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(key)) => HttpResponse::Created().json(key),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `GET /api/keys/{id}`，按 ID 返回单个密钥。
pub async fn get_key(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let id = path.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<Option<vortex_store::db::models::ApiKey>, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_keys::get_by_id(&conn, &id).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(Some(key))) => HttpResponse::Ok().json(key),
        Ok(Ok(None)) => HttpResponse::NotFound().json(json!({ "error": "Key not found" })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `DELETE /api/keys/{id}`，删除密钥；不存在时返回 404。
pub async fn delete_key(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let id = path.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_keys::delete(&conn, &id).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(true)) => HttpResponse::Ok().json(json!({ "success": true })),
        Ok(Ok(false)) => HttpResponse::NotFound().json(json!({ "error": "Key not found" })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
