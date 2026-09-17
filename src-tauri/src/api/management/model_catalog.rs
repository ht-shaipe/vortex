//! 模型目录管理端点。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

pub async fn list_catalog(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        crate::db::model_catalog::list_all(&conn)
            .map(|entries| json!({ "entries": entries }))
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// 从已配置的活跃连接自动拉取 `/v1/models` 端点，合并到模型目录。
pub async fn refresh_from_connections_handler(
    state: web::Data<std::sync::Arc<AppState>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let enc_key = state.encryption_key.clone();
    let provider_defaults: std::collections::HashMap<String, (String, String)> = state
        .provider_registry
        .list()
        .into_iter()
        .map(|d| (d.id.clone(), (d.base_url.clone(), d.models_path.clone())))
        .collect();
    match crate::providers::catalog_sync::refresh_from_connections(&pool, &enc_key, &provider_defaults).await {
        Ok((success, total, results)) => HttpResponse::Ok().json(json!({
            "successConnections": success,
            "totalModels": total,
            "results": results,
        })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("从连接刷新失败: {}", e) })),
    }
}
