use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, usage as db_usage};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn get_usage(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let limit: i64 = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);
    match db_usage::list_recent(&conn, limit) {
        Ok(entries) => HttpResponse::Ok().json(json!({"usage": entries})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn get_stats(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let since = query.get("since").map(|s| s.as_str());
    match db_usage::get_stats(&conn, since) {
        Ok(stats) => HttpResponse::Ok().json(stats),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}
