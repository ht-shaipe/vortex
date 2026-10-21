//! ToS 合规审查端点。
//!
//! `GET /api/tos-reviews`：返回所有提供商的 ToS（服务条款）审查结论，
//! 供前端展示各平台是否允许通过 API/浏览器自动化方式接入。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

/// 处理 `GET /api/tos-reviews`，按提供商名称排序列出全部审查记录。
pub async fn list_tos_reviews(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::tos_review::list_all(&conn)
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
