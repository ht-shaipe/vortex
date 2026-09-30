//! 健康检查端点
//!
//! 同步 DB 操作通过 `spawn_blocking` 移至阻塞线程池，避免阻塞 actix async runtime。

use actix_web::{HttpRequest, HttpResponse, web};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn health_check(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
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
        // 端口信息：https_port 为 null 表示未启用 HTTPS
        "http_port": state.config.port,
        "https_port": if state.config.tls_enabled {
            serde_json::Value::from(state.config.tls_https_port)
        } else {
            serde_json::Value::Null
        },
    }))
}

/// 根路径索引：浏览器直接访问网关时返回版本标识，同时便于连通性确认。
///
/// `v` 查询参数（如 `/?pretty=1`）预留；当前固定输出纯文本 `vortex vX.Y.Z`。
pub async fn index() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(format!("vortex v{}", env!("CARGO_PKG_VERSION")))
}

/// 根路径下的任意未知路径同样返回版本标识（避免浏览器 404 白页）。
pub async fn index_fallback(req: HttpRequest) -> HttpResponse {
    let _ = req;
    index().await
}
