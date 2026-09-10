//! 设置管理端点
//!
//! 提供全局设置的读取与更新接口，供前端 UI 管理应用级配置。
//! 所有操作委托 [`db_settings`](crate::db::settings) 数据库层完成。

use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, settings as db_settings};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

/// 处理 GET 请求，获取当前全局设置。
///
/// - `state`：应用全局状态
/// - 返回值：设置 JSON 对象或 500 错误
pub async fn get_settings(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 读取设置
    match db_settings::get_settings(&conn) {
        Ok(settings) => HttpResponse::Ok().json(settings),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

/// 处理 PUT/POST 请求，更新全局设置。
///
/// - `state`：应用全局状态
/// - `body`：包含新设置值的 JSON 请求体
/// - 返回值：更新后的设置 JSON 对象或 500 错误
pub async fn update_settings(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    // 获取数据库连接
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    // 更新设置
    match db_settings::update_settings(&conn, &body) {
        Ok(settings) => HttpResponse::Ok().json(settings),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}
