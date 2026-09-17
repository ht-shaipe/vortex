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

/// 手动同步：先导入内置种子快照（保证基础数据可用），
/// 若配置了远程 feed 则再拉取远程数据覆盖更新。
pub async fn sync_catalog_handler(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();

    // 1. 导入内置种子（无条件 UPSERT，刷新为当前版本的快照数据）
    let builtin = crate::providers::catalog_sync::seed_builtin_catalog(&pool).await;
    let mut total = match builtin {
        Ok(n) => n,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("内置目录导入失败: {}", e) }));
        }
    };

    // 2. 若配置了远程 feed，拉取远程数据覆盖更新
    let feed_url = state.config.catalog_feed_url.clone();
    let mut warning: Option<String> = None;
    if !feed_url.is_empty() {
        match crate::providers::catalog_sync::sync_catalog(&pool, &feed_url).await {
            Ok(n) => total = n,
            Err(e) => {
                // 远程拉取失败不视为整体失败：内置数据已导入
                log::warn!("远程 feed 同步失败: {}", e);
                warning = Some(format!("远程 feed 同步失败：{}", e));
            }
        }
    }

    let mut body = json!({ "synced": total });
    if let Some(w) = warning {
        body["warning"] = json!(w);
    }
    HttpResponse::Ok().json(body)
}

/// 从已配置的活跃连接自动拉取 `/v1/models` 端点，合并到模型目录。
pub async fn refresh_from_connections_handler(
    state: web::Data<std::sync::Arc<AppState>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let enc_key = state.encryption_key.clone();
    match crate::providers::catalog_sync::refresh_from_connections(&pool, &enc_key).await {
        Ok((success, total, results)) => HttpResponse::Ok().json(json!({
            "successConnections": success,
            "totalModels": total,
            "results": results,
        })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("从连接刷新失败: {}", e) })),
    }
}
