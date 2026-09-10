use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;
use std::path::PathBuf;

use crate::error::{AppError, Result};

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn init_pool(data_dir: &PathBuf, encryption_key: String) -> Result<DbPool> {
    std::fs::create_dir_all(data_dir).map_err(|e| AppError::Internal(format!("Failed to create data dir: {}", e)))?;

    let db_path = data_dir.join("vortex.db");
    let enc_key = encryption_key;
    let manager = SqliteConnectionManager::file(&db_path)
        .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)
        .with_init(move |conn| {
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
            let _ = enc_key;
            Ok(())
        });

    let pool = Pool::builder()
        .max_size(8)
        .min_idle(Some(2))
        .build(manager)
        .map_err(|e| AppError::Internal(format!("Failed to create DB pool: {}", e)))?;

    Ok(pool)
}

pub fn run_migrations(pool: &DbPool) -> Result<()> {
    let conn = pool.get().map_err(|e| AppError::Internal(format!("Failed to get DB connection: {}", e)))?;
    crate::db::migration::run_migrations(&conn)?;
    Ok(())
}

pub fn get_conn(pool: &DbPool) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
    pool.get().map_err(|e| AppError::Internal(format!("Failed to get DB connection: {}", e)))
}
