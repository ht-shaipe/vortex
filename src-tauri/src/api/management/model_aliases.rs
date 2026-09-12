//! 模型别名（虚拟模型映射）管理端点。
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

pub async fn list_aliases(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        crate::db::model_aliases::list(&conn)
            .map(|aliases| json!({ "aliases": aliases }))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

pub async fn create_alias(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<crate::db::models::CreateModelAlias>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<crate::db::models::ModelAlias, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        crate::db::model_aliases::create(&conn, &req).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(alias)) => HttpResponse::Ok().json(alias),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

pub async fn update_alias(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
    body: web::Json<crate::db::models::UpdateModelAlias>,
) -> HttpResponse {
    let id = path.into_inner();
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<Option<crate::db::models::ModelAlias>, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        crate::db::model_aliases::update(&conn, &id, &req).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(Some(alias))) => HttpResponse::Ok().json(alias),
        Ok(Ok(None)) => HttpResponse::NotFound().json(json!({ "error": "Alias not found" })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

pub async fn delete_alias(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        crate::db::model_aliases::delete(&conn, &id).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({ "ok": true })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
