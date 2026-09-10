use actix_web::{web, HttpResponse};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn health_check(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    // 注意：rusqlite 的 `execute` 只接受不返回结果集的语句，
    // 对 `SELECT 1` 这类会返回行的语句必然报 ExecuteReturnedResults，
    // 必须用 `query_row` 才能真正探测连通性。
    let db_ok = match crate::db::core::get_conn(&state.db_pool) {
        Ok(conn) => conn
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .is_ok(),
        Err(_) => false,
    };

    HttpResponse::Ok().json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "database": if db_ok { "ok" } else { "error" },
    }))
}
