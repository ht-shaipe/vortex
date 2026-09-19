//! 速率限制管理端点。
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

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
