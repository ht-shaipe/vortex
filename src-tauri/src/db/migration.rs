//! 简单数据库迁移系统。
//!
//! 通过 `_vortex_migrations` 表跟踪已应用的迁移版本。启动时依次检查每个迁移脚本，
//! 若尚未应用则执行 SQL 并记录版本号，保证数据库 schema 始终处于最新状态。

use rusqlite::{Connection, params};
use crate::error::Result;

/// 迁移脚本列表：(版本号, SQL 内容)。
/// SQL 通过 `include_str!` 在编译期嵌入，无需运行时读取文件。
const MIGRATIONS: &[(&str, &str)] = &[
    ("001", include_str!("migrations/001_initial.sql")),
    ("002", include_str!("migrations/002_free_token_sites.sql")),
    ("003", include_str!("migrations/003_free_token_web_only.sql")),
    ("004", include_str!("migrations/004_provider_latency.sql")),
    ("005", include_str!("migrations/005_usage_index.sql")),
    ("006", include_str!("migrations/006_model_aliases.sql")),
];

/// 执行数据库迁移。
///
/// 首先确保 `_vortex_migrations` 跟踪表存在，然后遍历所有迁移脚本：
/// 对于尚未记录的版本，执行其 SQL 并在跟踪表中插入记录。
///
/// # 参数
/// - `conn`：SQLite 数据库连接引用
///
/// # 返回
/// 成功返回 `Ok(())`，失败返回对应的错误
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // 创建迁移跟踪表（若不存在）
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _vortex_migrations (
            version TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    )?;

    // 依次检查并应用每个迁移
    for (version, sql) in MIGRATIONS {
        // 查询该版本是否已应用
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM _vortex_migrations WHERE version = ?1",
            params![version],
            |row| row.get(0),
        )?;

        if !exists {
            // 执行迁移 SQL
            conn.execute_batch(sql)?;
            // 记录已应用的版本
            conn.execute(
                "INSERT INTO _vortex_migrations (version, name) VALUES (?1, ?2)",
                params![version, format!("migration_{}", version)],
            )?;
        }
    }

    Ok(())
}
