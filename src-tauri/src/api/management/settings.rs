//! 设置管理端点
//!
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, settings as db_settings};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn get_settings(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_settings::get_settings(&conn)
            .map(|s| serde_json::to_value(s).unwrap_or(json!({})))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

pub async fn update_settings(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let body = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = db_core::get_conn(&pool).map_err(|e| e.to_string())?;
        db_settings::update_settings(&conn, &body)
            .map(|s| serde_json::to_value(s).unwrap_or(json!({})))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
