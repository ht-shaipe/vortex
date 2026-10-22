//! 存储维护端点：大表（压缩存档/请求日志）限额清理与运行日志管理。
//!
//! 统计数据（usage_history）全保留；自动巡检在网关启动任务里
//! （每 6 小时），此处提供查询与手动触发入口。
//! 运行日志（logs/vortex.log，5MB 自动截断）提供大小统计与手动清空。

use crate::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use vortex_store::db::maintenance::{budget_mb_from_env, enforce_storage_budget, storage_stats};

/// GET /api/maintenance/storage：当前存储占用统计与预算配置
///
/// 含运行日志文件大小（`logFileBytes`，数据目录 `logs/vortex.log`）。
pub async fn storage_info(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let pool = state.db_pool.clone();
    let data_dir = state.config.data_dir.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let stats = storage_stats(&conn).map_err(|e| e.to_string())?;
        let log_file_bytes = crate::logging::log_file_size(&data_dir);
        Ok(json!({
            "stats": stats,
            "budgetMb": budget_mb_from_env(),
            "logFileBytes": log_file_bytes,
            "logMaxBytes": crate::logging::MAX_LOG_FILE_BYTES,
        }))
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/maintenance/storage/cleanup 请求体
#[derive(Deserialize)]
pub struct StorageCleanupRequest {
    /// true 只统计不删除（前端先预览，用户确认后传 false 执行）
    #[serde(default)]
    pub dry_run: bool,
}

/// POST /api/maintenance/storage/cleanup：手动执行存储预算清理。
///
/// 与自动巡检同一实现（先删过期存档 → 超预算从旧到新删 → VACUUM）。
pub async fn storage_cleanup(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<StorageCleanupRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let dry_run = body.dry_run;
    let budget_mb = budget_mb_from_env();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let report = enforce_storage_budget(&conn, budget_mb * 1024 * 1024, dry_run)
            .map_err(|e| e.to_string())?;
        Ok(json!({ "budgetMb": budget_mb, "report": report }))
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

/// POST /api/maintenance/log/clear：清空运行日志文件
///
/// 返回清理前后大小（字节）。日志写入器持有的句柄不受影响，继续可写。
pub async fn log_clear(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let data_dir = state.config.data_dir.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let before_bytes = crate::logging::clear_log_file(&data_dir)
            .map_err(|e| e.to_string())?;
        let after_bytes = crate::logging::log_file_size(&data_dir);
        Ok(json!({
            "beforeBytes": before_bytes,
            "afterBytes": after_bytes,
        }))
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
