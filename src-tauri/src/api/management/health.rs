use actix_web::{web, HttpResponse};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn health_check(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let db_ok = match crate::db::core::get_conn(&state.db_pool) {
        Ok(conn) => conn.execute("SELECT 1", []).is_ok(),
        Err(_) => false,
    };

    HttpResponse::Ok().json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "database": if db_ok { "ok" } else { "error" },
    }))
}
