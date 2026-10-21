//! 模型目录管理端点。

use crate::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

/// 处理 `GET /api/model-catalog`，返回远程同步的模型目录。
pub async fn list_catalog(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        vortex_store::db::model_catalog::list_all(&conn)
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
    match vortex_router::providers::catalog_sync::refresh_from_connections(
        &pool,
        &enc_key,
        &provider_defaults,
    )
    .await
    {
        Ok((success, total, results)) => HttpResponse::Ok().json(json!({
            "successConnections": success,
            "totalModels": total,
            "results": results,
        })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("从连接刷新失败: {}", e) })),
    }
}

/// POST /api/model-catalog/cleanup 请求体
#[derive(Deserialize)]
pub struct CleanupRequest {
    /// true 只统计不删除（前端先预览，用户确认后传 false 执行）
    #[serde(default)]
    pub dry_run: bool,
}

/// 处理 `POST /api/model-catalog/cleanup`，清理删除连接后的残留数据。
///
/// 两类清理：
/// 1. 孤儿模型目录：删除「provider 无对应连接」的目录记录（连接删除后
///    sync 只 upsert 不删，记录会一直残留）。归属规则与 catalog_sync
///    一致：连接的 customId（若有）或连接 provider 名；包含未激活连接
///    （临时停用的连接重新启用时目录可直接复用）
/// 2. 悬空别名 target：connection_id 指向已删连接的 target 剔除；
///    剔空的 auto 别名整条删除（自动归纳可重建），manual 别名保留条目
///
/// 建议前端先 `dryRun=true` 预览，用户确认后传 `dryRun=false` 执行。
pub async fn cleanup_catalog(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<CleanupRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let dry_run = body.dry_run;
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        // 一次读连接表：目录保留集合（customId 或 provider）
        // 与别名存活连接 ID 集合
        let mut keep = std::collections::HashSet::new();
        let mut live_conn_ids = std::collections::HashSet::new();
        {
            let mut stmt = conn
                .prepare("SELECT id, provider, provider_specific_data FROM provider_connections")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for (id, provider, pdata) in rows
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
            {
                live_conn_ids.insert(id);
                let custom_id = pdata
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .and_then(|v| {
                        v.get("customId")
                            .and_then(|x| x.as_str())
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    });
                // customId 与 conn.provider 两者都保留：现版 sync 按 customId
                // 归属，但存量目录里有按 conn.provider（如 custom-openai）
                // 写入的历史记录（ON CONFLICT(provider,model) 下两者不互通）
                if let Some(cid) = custom_id {
                    keep.insert(cid);
                }
                keep.insert(provider);
            }
        }
        let keep: Vec<String> = keep.into_iter().collect();
        let stats = vortex_store::db::model_catalog::cleanup_orphans(&conn, &keep, dry_run)
            .map_err(|e| e.to_string())?;
        let total: i64 = stats.iter().map(|(_, n)| n).sum();
        let providers: Vec<serde_json::Value> = stats
            .into_iter()
            .map(|(provider, count)| json!({ "provider": provider, "count": count }))
            .collect();
        // 悬空别名 target 清理（与目录清理同一 dry_run 语义）
        let (alias_affected, alias_removed, alias_deleted) =
            vortex_store::db::model_aliases::cleanup_dangling_targets(
                &conn,
                &live_conn_ids,
                dry_run,
            )
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "dryRun": dry_run,
            "totalDeleted": total,
            "providers": providers,
            "aliases": {
                "affected": alias_affected,
                "removedTargets": alias_removed,
                "deletedAutoAliases": alias_deleted,
            },
        }))
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
