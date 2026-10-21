//! 加密数据库备份端点。
//!
//! - `POST /api/backups`：使用 SQLite `VACUUM INTO` 生成一致性快照
//! - `GET  /api/backups`：扫描数据目录下的备份文件
//!
//! 备份是完整的 SQLite 数据库文件；库内的 API Key 等敏感字段本身已加密存储。

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::AppState;

/// 创建备份请求体。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    /// 备份目录路径（可选，默认数据目录）
    backup_dir: Option<String>,
}

/// 备份结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    /// 是否成功（失败会以 HTTP 错误返回，成功时恒为 true）
    success: bool,
    /// 生成的备份文件绝对路径
    backup_path: String,
    /// 备份文件大小（字节）
    file_size: u64,
    /// 备份是否包含加密存储的敏感字段（当前实现固定为 true）
    encrypted: bool,
}

/// 处理 `POST /api/backups`，生成带时间戳的数据库快照。
///
/// 文件名格式：`vortex_backup_YYYYMMDD_HHMMSS.db`。
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

        // VACUUM INTO 生成一致性快照，等价于 SQLite backup API，且无需额外句柄
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

/// 处理 `GET /api/backups`，列出数据目录下的 `vortex_backup_*.db` 文件。
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
