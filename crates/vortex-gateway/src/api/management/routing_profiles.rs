//! 路由配置文件管理端点。
//!
//! 路由配置（routing profile）是一组有序的目标（provider + model + 可选连接），
//! 供命名回退链按顺序尝试。端点：
//! - `GET  /api/routing-profiles`：列出全部配置
//! - `POST /api/routing-profiles`：创建配置
//! - `DELETE /api/routing-profiles/{id}`：删除配置

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

/// 处理 `GET /api/routing-profiles`，按名称排序返回全部路由配置。
pub async fn list_profiles(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::routing_profiles::list(&conn)
            .map(|profiles| json!({ "profiles": profiles }))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `POST /api/routing-profiles`，校验并创建路由配置。
pub async fn create_profile(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<vortex_store::db::routing_profiles::CreateRoutingProfile>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<vortex_store::db::routing_profiles::RoutingProfile, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::routing_profiles::create(&conn, &req).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(profile)) => HttpResponse::Ok().json(profile),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 处理 `DELETE /api/routing-profiles/{id}`，按 ID 删除路由配置。
pub async fn delete_profile(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::routing_profiles::delete(&conn, &id).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({ "ok": true })),
        Ok(Err(e)) => HttpResponse::BadRequest().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
