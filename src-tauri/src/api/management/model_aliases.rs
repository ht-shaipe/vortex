//! 模型别名（虚拟模型映射）管理端点。

use actix_web::{web, HttpResponse};
use serde_json::json;

use crate::AppState;

/// 列出所有模型别名。
pub async fn list_aliases(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let Ok(conn) = crate::db::core::get_conn(&pool) else {
        return HttpResponse::InternalServerError().json(json!({"error": "Database connection failed"}));
    };
    match crate::db::model_aliases::list(&conn) {
        Ok(aliases) => HttpResponse::Ok().json(json!({ "aliases": aliases })),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

/// 创建模型别名。
pub async fn create_alias(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<crate::db::models::CreateModelAlias>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let Ok(conn) = crate::db::core::get_conn(&pool) else {
        return HttpResponse::InternalServerError().json(json!({"error": "Database connection failed"}));
    };
    match crate::db::model_aliases::create(&conn, &body.into_inner()) {
        Ok(alias) => HttpResponse::Ok().json(alias),
        Err(e) => HttpResponse::BadRequest().json(json!({"error": e.to_string()})),
    }
}

/// 更新模型别名。
pub async fn update_alias(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
    body: web::Json<crate::db::models::UpdateModelAlias>,
) -> HttpResponse {
    let id = path.into_inner();
    let pool = state.db_pool.clone();
    let Ok(conn) = crate::db::core::get_conn(&pool) else {
        return HttpResponse::InternalServerError().json(json!({"error": "Database connection failed"}));
    };
    match crate::db::model_aliases::update(&conn, &id, &body.into_inner()) {
        Ok(Some(alias)) => HttpResponse::Ok().json(alias),
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Alias not found"})),
        Err(e) => HttpResponse::BadRequest().json(json!({"error": e.to_string()})),
    }
}

/// 删除模型别名。
pub async fn delete_alias(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    let pool = state.db_pool.clone();
    let Ok(conn) = crate::db::core::get_conn(&pool) else {
        return HttpResponse::InternalServerError().json(json!({"error": "Database connection failed"}));
    };
    match crate::db::model_aliases::delete(&conn, &id) {
        Ok(_) => HttpResponse::Ok().json(json!({"ok": true})),
        Err(e) => HttpResponse::BadRequest().json(json!({"error": e.to_string()})),
    }
}
