//! API 密钥管理端点
//!
//! 提供本地 API 密钥的增删查接口，供前端 UI 管理用于鉴权的密钥。
//! 所有操作直接委托 [`db_keys`](crate::db::api_keys) 数据库层完成。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, api_keys as db_keys};
use crate::db::models::CreateApiKeyRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 GET 请求，列出所有 API 密钥。
///
/// - `state`：应用全局状态
/// - 返回值：`{ "keys": [...] }` 格式 JSON 响应
pub async fn list_keys(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 查询全部密钥列表
    match db_keys::list(&conn) {
        Ok(keys) => HttpResponse::Ok().json(json!({"keys": keys})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

/// 处理 POST 请求，创建新的 API 密钥。
///
/// - `state`：应用全局状态
/// - `body`：创建密钥请求体（含名称、权限等）
/// - 返回值：201 Created 含新建密钥对象，或 500 错误
pub async fn create_key(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateApiKeyRequest>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 创建密钥
    match db_keys::create(&conn, &body) {
        Ok(key) => HttpResponse::Created().json(key),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

/// 处理 GET 请求，按 ID 获取单个 API 密钥。
///
/// - `state`：应用全局状态
/// - `path`：URL 路径中的密钥 ID
/// - 返回值：密钥对象、404 未找到或 500 错误
pub async fn get_key(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner(); // 提取路径参数中的 ID
    // 按 ID 查询密钥
    match db_keys::get_by_id(&conn, &id) {
        Ok(Some(key)) => HttpResponse::Ok().json(key),
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Key not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

/// 处理 DELETE 请求，按 ID 删除 API 密钥。
///
/// - `state`：应用全局状态
/// - `path`：URL 路径中的密钥 ID
/// - 返回值：`{ "success": true }`、404 未找到或 500 错误
pub async fn delete_key(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner(); // 提取路径参数中的 ID
    // 删除密钥
    match db_keys::delete(&conn, &id) {
        Ok(true) => HttpResponse::Ok().json(json!({"success": true})),
        Ok(false) => HttpResponse::NotFound().json(json!({"error": "Key not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}
