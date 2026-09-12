//! 健康检查端点
//!
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{web, HttpResponse};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn health_check(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        Ok(conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0)).is_ok())
    })
    .await;

    let db_ok = match result {
        Ok(Ok(v)) => v,
        _ => false,
    };

    HttpResponse::Ok().json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "database": if db_ok { "ok" } else { "error" },
    }))
}
