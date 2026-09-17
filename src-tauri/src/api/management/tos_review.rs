//! ToS 合规审查端点。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

pub async fn list_tos_reviews(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        crate::db::tos_review::list_all(&conn)
            .map(|reviews| json!({ "reviews": reviews }))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
