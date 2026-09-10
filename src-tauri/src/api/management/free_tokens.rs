use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, free_tokens as db_free_tokens};
use crate::db::models::CreateFreeTokenSiteRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn list_sites(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    match db_free_tokens::list(&conn) {
        Ok(sites) => HttpResponse::Ok().json(json!({"sites": sites})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn create_site(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateFreeTokenSiteRequest>,
) -> HttpResponse {
    if body.name.trim().is_empty() {
        return HttpResponse::BadRequest().json(json!({"error": "站点名称不能为空"}));
    }

    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    match db_free_tokens::create(&conn, &body) {
        Ok(site) => HttpResponse::Created().json(site),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn delete_site(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_free_tokens::delete_user_site(&conn, &id) {
        Ok(true) => HttpResponse::Ok().json(json!({"success": true})),
        Ok(false) => HttpResponse::NotFound().json(json!({"error": "Site not found"})),
        Err(e) => HttpResponse::BadRequest().json(json!({"error": e.to_string()})),
    }
}
