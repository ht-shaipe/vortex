//! 内容寻址回忆端点：查看和检索被压缩的原始 prompt 内容。
//!
//! 压缩管线以 SHA-256 为键保存原文，前端可通过 hash 找回压缩前的内容。
//! - `GET /api/compressed-content`：最近 50 条记录（不含原文）
//! - `GET /api/compressed-content/{hash}`：按 hash 取完整原文
use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

/// 处理 `GET /api/compressed-content`，列出最近的压缩内容记录（原文为空）。
pub async fn list_compressed_content(
    state: web::Data<std::sync::Arc<AppState>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::compressed_content::list_recent(&conn, 50)
            .map(|entries| json!({ "entries": entries }))
            .map_err(|e| e.to_string())
    }).await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 按 hash 检索原始内容
pub async fn get_compressed_content(
    state: web::Data<std::sync::Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let hash = path.into_inner();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::compressed_content::retrieve(&conn, &hash)
            .map(|entry| match entry {
                Some(e) => json!({ "entry": e }),
                None => json!({ "error": "未找到" }),
            })
            .map_err(|e| e.to_string())
    }).await;
    match result {
        Ok(Ok(v)) => {
            if v.get("error").is_some() {
                HttpResponse::NotFound().json(v)
            } else {
                HttpResponse::Ok().json(v)
            }
        }
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
