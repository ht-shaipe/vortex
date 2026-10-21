//! 存储预算维护：控制日志/详情大表占用的磁盘空间。
//!
//! 设计：
//! - **统计全保留**：`usage_history`（用量统计）行数小、是账本，永不因
//!   预算被删；
//! - **大表限额**：`compressed_content`（压缩前的原始 prompt 存档，
//!   单行可达数百 KB，历史上撑到 700MB+）与 `call_logs`（请求日志，
//!   含 request_summary 等大字段）是膨胀源；
//! - **执行顺序**：先删已过期存档 → 仍超预算则按 created_at 从旧到新
//!   删存档行 → call_logs 先清空大字段再删旧行 → `VACUUM` 收缩文件
//!   （删行不缩文件，必须 VACUUM）。
//!
//! 预算默认 200MB（`VORTEX_STORAGE_BUDGET_MB` 可覆盖，下限 50MB）。
//! 自动任务在网关启动后 2 分钟首跑、之后每 6 小时巡检（见 gateway
//! lib.rs）；也可经 `POST /api/maintenance/storage/cleanup` 手动触发。

use crate::error::Result;
use rusqlite::Connection;
use serde::Serialize;

/// 默认存储预算（MB）
pub const DEFAULT_BUDGET_MB: i64 = 200;
/// 预算下限（MB）：防止误配成极小值把库清空
pub const MIN_BUDGET_MB: i64 = 50;

/// 从环境变量读预算（MB），缺省/非法回落默认值
pub fn budget_mb_from_env() -> i64 {
    std::env::var("VORTEX_STORAGE_BUDGET_MB")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .map(|v| v.max(MIN_BUDGET_MB))
        .unwrap_or(DEFAULT_BUDGET_MB)
}

/// 各表占用统计
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    /// 数据库文件大小（字节，含 WAL 外的 .db 主文件）
    pub db_file_bytes: i64,
    /// compressed_content 行数
    pub compressed_rows: i64,
    /// compressed_content original_content 总字节（估算）
    pub compressed_bytes: i64,
    /// call_logs 行数
    pub call_log_rows: i64,
    /// call_logs 大字段（request_summary + error_summary）总字节（估算）
    pub call_log_detail_bytes: i64,
    /// usage_history 行数（统计账本，永不因预算清理）
    pub usage_rows: i64,
}

/// 清理动作报告
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetReport {
    /// 执行前文件大小
    pub before_bytes: i64,
    /// 执行后文件大小（dry_run 时等于 before）
    pub after_bytes: i64,
    /// 本次删除的过期存档行数（expires_at 已过）
    pub expired_deleted: i64,
    /// 因预算删除的存档行数（旧→新）
    pub compressed_deleted: i64,
    /// call_logs 清空大字段的行数
    pub call_log_trimmed: i64,
    /// call_logs 删除的旧行数
    pub call_log_deleted: i64,
    /// 是否执行了 VACUUM
    pub vacuumed: bool,
    /// dry_run 标识
    pub dry_run: bool,
}

/// 数据库文件大小（字节）。路径未知时从连接取（journal 模式下的主文件）。
fn db_file_size(conn: &Connection) -> i64 {
    // PRAGMA 路径含 URI 参数时 metadata 失败，回落 page_count × page_size
    if let (Ok(size), Ok(page)) = (
        conn.query_row("PRAGMA page_count", [], |r| r.get::<_, i64>(0)),
        conn.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0)),
    ) {
        return size * page;
    }
    0
}

/// 表是否存在（call_logs 为历史遗留表：当前代码已无读写，但老用户库
/// 中可能有数据，处理前先探测）
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        rusqlite::params![name],
        |_| Ok(()),
    )
    .is_ok()
}

/// 收集存储统计（只读，供 API 与巡检判断）
pub fn storage_stats(conn: &Connection) -> Result<StorageStats> {
    let (compressed_rows, compressed_bytes) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(content_size), 0) FROM compressed_content",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (call_log_rows, call_log_detail_bytes) = if table_exists(conn, "call_logs") {
        conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(COALESCE(request_summary,'')) + LENGTH(COALESCE(error_summary,''))), 0) FROM call_logs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
    } else {
        (0, 0)
    };
    let usage_rows: i64 = conn.query_row("SELECT COUNT(*) FROM usage_history", [], |r| r.get(0))?;
    Ok(StorageStats {
        db_file_bytes: db_file_size(conn),
        compressed_rows,
        compressed_bytes,
        call_log_rows,
        call_log_detail_bytes,
        usage_rows,
    })
}

/// 执行存储预算：清理过期存档（无条件），超预算时按序清理大表并
/// VACUUM 收缩。
///
/// `dry_run = true` 只统计将删除的行数，不改动。返回报告（含清理后大小）。
pub fn enforce_storage_budget(
    conn: &Connection,
    budget_bytes: i64,
    dry_run: bool,
) -> Result<BudgetReport> {
    let before = db_file_size(conn);
    let mut report = BudgetReport {
        before_bytes: before,
        after_bytes: before,
        dry_run,
        ..Default::default()
    };

    // 1. 删已过期存档（无条件执行，不受预算门控——过期即废）。
    //    datetime() 归一化比较：RFC3339 与 SQLite 格式混存均可解析
    let expired: i64 = conn.query_row(
        "SELECT COUNT(*) FROM compressed_content \
         WHERE expires_at IS NOT NULL AND datetime(expires_at) < datetime('now')",
        [],
        |r| r.get(0),
    )?;
    if expired > 0 {
        report.expired_deleted = expired;
        if !dry_run {
            conn.execute(
                "DELETE FROM compressed_content \
                 WHERE expires_at IS NOT NULL AND datetime(expires_at) < datetime('now')",
                [],
            )?;
        }
    }

    let mut size_now = db_file_size(conn);
    if size_now <= budget_bytes {
        // 预算内（可能刚清了过期）：仅在删过期时也需要 VACUUM 收缩
        if !dry_run && expired > 0 {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
            conn.execute_batch("VACUUM")?;
            report.vacuumed = true;
            report.after_bytes = db_file_size(conn);
        }
        return Ok(report);
    }

    // 2. 超预算 → 按 created_at 旧→新删存档，直到估算大小落入预算。
    // 估算：文件大小与大字段总量强相关（历史实测 749MB 文件 ≈ 747MB 存档），
    // 需再释放 = before - budget + 10% 余量（覆盖索引/页开销与估算误差）
    {
        let need = size_now - budget_bytes + budget_bytes / 10;
        let rows: Vec<String> = conn
            .prepare(
                "SELECT hash FROM compressed_content ORDER BY datetime(created_at) ASC, content_size DESC",
            )?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut freed: i64 = 0;
        let mut to_delete = Vec::new();
        for hash in rows {
            let sz: i64 = conn.query_row(
                "SELECT content_size FROM compressed_content WHERE hash = ?1",
                rusqlite::params![hash],
                |r| r.get(0),
            )?;
            to_delete.push(hash);
            freed += sz;
            if freed >= need {
                break;
            }
        }
        report.compressed_deleted = to_delete.len() as i64;
        if !dry_run && !to_delete.is_empty() {
            let mut stmt = conn.prepare("DELETE FROM compressed_content WHERE hash = ?1")?;
            for hash in &to_delete {
                stmt.execute(rusqlite::params![hash])?;
            }
        }
        size_now = db_file_size(conn);
    }

    // 3. call_logs 防御（历史遗留表，可能不存在）：先清大字段（保留行
    //    骨架），仍超预算再删旧行
    if size_now > budget_bytes && table_exists(conn, "call_logs") {
        let trimmed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM call_logs \
             WHERE LENGTH(COALESCE(request_summary,'')) + LENGTH(COALESCE(error_summary,'')) > 0",
            [],
            |r| r.get(0),
        )?;
        if trimmed > 0 {
            report.call_log_trimmed = trimmed;
            if !dry_run {
                conn.execute(
                    "UPDATE call_logs SET request_summary = NULL, error_summary = NULL",
                    [],
                )?;
            }
        }
        size_now = db_file_size(conn);
        if size_now > budget_bytes {
            let deleted: i64 =
                conn.query_row("SELECT COUNT(*) FROM call_logs", [], |r| r.get(0))?;
            report.call_log_deleted = deleted;
            if !dry_run && deleted > 0 {
                conn.execute("DELETE FROM call_logs", [])?;
            }
        }
    }

    // 4. VACUUM 收缩（删行不缩文件）。WAL 先 checkpoint 清空，再整库重写
    if !dry_run
        && report.expired_deleted
            + report.compressed_deleted
            + report.call_log_trimmed
            + report.call_log_deleted
            > 0
    {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        conn.execute_batch("VACUUM")?;
        report.vacuumed = true;
        report.after_bytes = db_file_size(conn);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&conn).unwrap();
        conn
    }

    fn add_archive(conn: &Connection, hash: &str, kb: usize, created: &str) {
        conn.execute(
            "INSERT INTO compressed_content (hash, original_content, content_size, saved_tokens, created_at, expires_at)
             VALUES (?1, ?2, ?3, 1, ?4, NULL)",
            rusqlite::params![hash, "x".repeat(kb * 1024), (kb * 1024) as i64, created],
        )
        .unwrap();
    }

    /// 预算内不动作；超预算删最旧存档；usage_history 永不删
    #[test]
    fn enforce_budget_deletes_oldest_first() {
        let conn = setup();
        add_archive(&conn, "old", 500, "2026-09-01 00:00:00");
        add_archive(&conn, "new", 100, "2026-10-01 00:00:00");
        conn.execute(
            "INSERT INTO usage_history (provider, model, tokens_input, tokens_output, timestamp)
             VALUES ('p', 'm', 1, 2, '2026-09-01')",
            [],
        )
        .unwrap();

        // 预算内：无动作
        let r = enforce_storage_budget(&conn, 10 * 1024 * 1024, false).unwrap();
        assert_eq!(r.compressed_deleted, 0);

        // 超预算：优先删最旧的 old（内存库含迁移表页开销，before 基线
        // 不确定——不断言"new 保留"这种精确结果，改为验证两个核心
        // 语义：最旧先行删除 + VACUUM 后实际大小落入预算（含余量））
        let r = enforce_storage_budget(&conn, 200 * 1024, false).unwrap();
        assert!(r.compressed_deleted >= 1);
        let remains: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM compressed_content WHERE hash = 'old'",
                [],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(remains, 0, "最旧存档应最先被删");
        assert!(r.vacuumed, "删行后必须 VACUUM 收缩");
        assert!(
            r.after_bytes <= 200 * 1024 + 128 * 1024,
            "清理后大小应落入预算（含页/索引开销余量），实际 {}",
            r.after_bytes
        );
        let usage: i64 = conn
            .query_row("SELECT COUNT(*) FROM usage_history", [], |x| x.get(0))
            .unwrap();
        assert_eq!(usage, 1, "用量统计永不因预算被删");
    }

    /// dry_run 只统计不动数据
    #[test]
    fn dry_run_does_not_delete() {
        let conn = setup();
        add_archive(&conn, "a", 500, "2026-09-01 00:00:00");
        let r = enforce_storage_budget(&conn, 100 * 1024, true).unwrap();
        assert!(r.compressed_deleted >= 1);
        assert!(r.dry_run);
        assert!(!r.vacuumed);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM compressed_content", [], |x| x.get(0))
            .unwrap();
        assert_eq!(rows, 1, "dry_run 不得删行");
    }

    /// 过期存档优先被清（含 RFC3339 带 +00:00 的 expires_at）
    #[test]
    fn expired_archives_cleaned_first() {
        let conn = setup();
        conn.execute(
            "INSERT INTO compressed_content (hash, original_content, content_size, created_at, expires_at)
             VALUES ('exp', 'x', 10, '2026-09-01 00:00:00', '2026-09-15T10:00:00+00:00')",
            [],
        )
        .unwrap();
        // 预算内（不触发预算删除路径），仅靠过期清理
        let r = enforce_storage_budget(&conn, 10 * 1024 * 1024, false).unwrap();
        assert_eq!(r.expired_deleted, 1, "过期存档应被清");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM compressed_content", [], |x| x.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }
}
