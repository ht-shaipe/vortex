use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, combos as db_combos};
use crate::db::models::CreateComboRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn list_combos(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    match db_combos::list(&conn) {
        Ok(combos) => HttpResponse::Ok().json(json!({"combos": combos})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn create_combo(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateComboRequest>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    match db_combos::create(&conn, &body) {
        Ok(combo) => HttpResponse::Created().json(combo),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn get_combo(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_combos::get_by_id(&conn, &id) {
        Ok(Some(combo)) => HttpResponse::Ok().json(combo),
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Combo not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn update_combo(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_combos::update(&conn, &id, &body) {
        Ok(Some(combo)) => HttpResponse::Ok().json(combo),
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Combo not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn delete_combo(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_combos::delete(&conn, &id) {
        Ok(true) => HttpResponse::Ok().json(json!({"success": true})),
        Ok(false) => HttpResponse::NotFound().json(json!({"error": "Combo not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}
