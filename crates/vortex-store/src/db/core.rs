//! SQLite 连接池初始化与连接获取。
//!
//! 基于 r2d2 连接池管理 SQLite 连接，配置最大 8 个连接、最少 2 个空闲连接，
//! 开启 WAL 日志模式、外键约束以及 5 秒忙超时，保证并发读写安全与数据完整性。

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;
use std::path::PathBuf;

use crate::error::{AppError, Result};

/// 数据库连接池类型别名，底层为 r2d2 管理的 SQLite 连接池。
pub type DbPool = Pool<SqliteConnectionManager>;

/// 初始化数据库连接池。
///
/// 在指定 `data_dir` 下创建（若不存在）`vortex.db` 数据库文件，
/// 并构建一个最大 8 连接、最小 2 空闲的 r2d2 连接池。每个新连接会自动执行
/// `PRAGMA journal_mode=WAL`（WAL 日志）、`PRAGMA foreign_keys=ON`（外键约束）、
/// `PRAGMA busy_timeout=5000`（5 秒忙超时）。
///
/// # 参数
/// - `data_dir`：数据库文件所在目录，不存在时自动创建
/// - `encryption_key`：加密密钥（当前仅捕获，预留供连接初始化使用）
///
/// # 返回
/// 成功返回 `DbPool`，失败返回 `AppError::Internal`
pub fn init_pool(data_dir: &PathBuf, encryption_key: String) -> Result<DbPool> {
    // 确保数据目录存在
    std::fs::create_dir_all(data_dir).map_err(|e| AppError::Internal(format!("Failed to create data dir: {}", e)))?;

    // 拼接数据库文件路径
    let db_path = data_dir.join("vortex.db");
    let enc_key = encryption_key;
    let manager = SqliteConnectionManager::file(&db_path)
        .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)
        .with_init(move |conn| {
            // 每个新连接初始化：WAL 模式 + 外键约束 + 5 秒忙超时
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
            let _ = enc_key;
            Ok(())
        });

    // 构建连接池：最大 8 连接，最小 2 空闲
    let pool = Pool::builder()
        .max_size(8)
        .min_idle(Some(2))
        .build(manager)
        .map_err(|e| AppError::Internal(format!("Failed to create DB pool: {}", e)))?;

    Ok(pool)
}

/// 执行数据库迁移。
///
/// 从连接池获取一个连接，然后调用 `migration::run_migrations` 应用所有待执行的迁移脚本。
///
/// # 参数
/// - `pool`：数据库连接池引用
///
/// # 返回
/// 成功返回 `Ok(())`，失败返回对应的 `AppError`
pub fn run_migrations(pool: &DbPool) -> Result<()> {
    let conn = pool.get().map_err(|e| AppError::Internal(format!("Failed to get DB connection: {}", e)))?;
    crate::db::migration::run_migrations(&conn)?;
    Ok(())
}

/// 从连接池获取一个可用连接。
///
/// # 参数
/// - `pool`：数据库连接池引用
///
/// # 返回
/// 成功返回一个 `PooledConnection`（用完自动归还池），失败返回 `AppError::Internal`
pub fn get_conn(pool: &DbPool) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
    pool.get().map_err(|e| AppError::Internal(format!("Failed to get DB connection: {}", e)))
}
