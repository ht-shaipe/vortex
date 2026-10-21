//! 速率限制管理端点。
//!
//! 端点：
//! - `GET    /api/rate-limits`：列出全部速率上限
//! - `POST   /api/rate-limits`：新增或更新上限（同时刷新内存限流器）
//! - `DELETE /api/rate-limits/{id}`：删除上限
//! - `GET    /api/rate-limits/usage`：查看当前窗口用量快照
//! - `POST   /api/rate-limits/cleanup`：清理过期用量记录
//!
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

/// 处理 `GET /api/rate-limits`，列出所有速率上限配置。
pub async fn list_rate_limits(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::rate_limits::list_caps(&conn)
            .map(|caps| json!({ "caps": caps }))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `POST /api/rate-limits`，写入上限后同步更新内存限流器。
///
/// 内存更新放在 `spawn_blocking` 内完成，保证 DB 与限流器状态一致。
pub async fn upsert_rate_limit(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<vortex_store::db::rate_limits::UpsertRateLimitCap>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let rate_limiter = state.rate_limiter.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<vortex_store::db::rate_limits::RateLimitCap, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let cap = vortex_store::db::rate_limits::upsert_cap(&conn, &req).map_err(|e| e.to_string())?;
        rate_limiter.set_cap(
            &cap.provider,
            &cap.model,
            cap.connection_id.as_deref(),
            cap.rpm,
            cap.rpd,
            cap.tpm,
            cap.tpd,
        );
        Ok(cap)
    })
    .await;
    match result {
        Ok(Ok(cap)) => HttpResponse::Ok().json(cap),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `DELETE /api/rate-limits/{id}`，删除指定速率上限。
pub async fn delete_rate_limit(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::rate_limits::delete_cap(&conn, &id).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({ "ok": true })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `GET /api/rate-limits/usage`，返回各 provider/model 的当前窗口用量快照。
pub async fn list_usage(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::rate_limits::list_usage_snapshots(&conn)
            .map(|snapshots| json!({ "snapshots": snapshots }))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `POST /api/rate-limits/cleanup`，清理已过期的用量计数记录。
pub async fn cleanup_usage(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let deleted = vortex_store::db::rate_limits::cleanup_old_usage(&conn).map_err(|e| e.to_string())?;
        Ok(json!({ "deleted": deleted }))
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
