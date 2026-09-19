//! 加密数据库备份端点。

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    /// 备份目录路径（可选，默认数据目录）
    backup_dir: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    success: bool,
    backup_path: String,
    file_size: u64,
    encrypted: bool,
}

pub async fn create_backup(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<BackupRequest>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let data_dir = state.config.data_dir.clone();
    let req = body.into_inner();

    let result = tokio::task::spawn_blocking(move || -> Result<BackupResult, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;

        let backup_dir = req.backup_dir.unwrap_or_else(|| data_dir.to_string_lossy().to_string());
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_path = format!("{}/vortex_backup_{}.db", backup_dir, timestamp);

        // 使用 SQLite 的 backup API
        conn.execute(&format!("VACUUM INTO '{}'", backup_path), [])
            .map_err(|e| e.to_string())?;

        let file_size = std::fs::metadata(&backup_path)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(BackupResult {
            success: true,
            backup_path,
            file_size,
            encrypted: true,
        })
    })
    .await;

    match result {
        Ok(Ok(r)) => HttpResponse::Ok().json(r),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

pub async fn list_backups(state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let data_dir = state.config.data_dir.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let mut backups = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("vortex_backup_") && name.ends_with(".db") {
                    let path = entry.path();
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let modified = entry.metadata().and_then(|m| m.modified()).ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    backups.push(json!({
                        "name": name,
                        "path": path.to_string_lossy(),
                        "size": size,
                        "modified": modified,
                    }));
                }
            }
        }
        Ok(json!({ "backups": backups }))
    })
    .await;

    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
