//! API Key 权限管理端点。
use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

/// 创建权限规则请求体。
#[derive(serde::Deserialize)]
pub struct CreatePermissionReq {
    /// 规则所属的 API Key ID
    pub api_key_id: String,
    /// 规则类型：`allow` 或 `deny`
    pub rule_type: String,
    /// 模型匹配模式，支持 `*` 通配（如 `gpt-*`）
    pub model_pattern: String,
}

/// 列出所有权限规则
pub async fn list_permissions(
    state: web::Data<std::sync::Arc<AppState>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::key_permissions::list_all(&conn)
            .map(|rules| json!({ "rules": rules }))
            .map_err(|e| e.to_string())
    }).await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 创建权限规则
pub async fn create_permission(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<CreatePermissionReq>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let req = body.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::key_permissions::create(&conn, &req.api_key_id, &req.rule_type, &req.model_pattern)
            .map(|rule| json!({ "rule": rule }))
            .map_err(|e| e.to_string())
    }).await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 删除权限规则
pub async fn delete_permission(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let id = path.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::key_permissions::delete(&conn, &id).map_err(|e| e.to_string())
    }).await;
    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(json!({ "ok": true })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
