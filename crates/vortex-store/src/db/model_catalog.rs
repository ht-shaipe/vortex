//! 远程模型目录 CRUD。
//!
//! 存储从远程 feed 拉取的模型信息，包括上下文窗口、速率限制、能力标签等。
//! 定时同步任务会定期拉取远程 feed 并更新此表。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 模型目录条目，对应 `model_catalog` 表
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpd: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpd: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_quota: Option<String>,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub capabilities: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_feed: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_synced: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 列出所有目录条目
pub fn list_all(conn: &Connection) -> Result<Vec<CatalogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model, name, context_window, rpm, rpd, tpm, tpd,
         free_quota, supports_tools, supports_streaming, supports_vision, capabilities,
         source_feed, is_active, last_synced, created_at, updated_at
         FROM model_catalog ORDER BY provider, model",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CatalogEntry {
            id: row.get(0)?,
            provider: row.get(1)?,
            model: row.get(2)?,
            name: row.get(3)?,
            context_window: row.get(4)?,
            rpm: row.get(5)?,
            rpd: row.get(6)?,
            tpm: row.get(7)?,
            tpd: row.get(8)?,
            free_quota: row.get(9)?,
            supports_tools: row.get::<_, i32>(10)? != 0,
            supports_streaming: row.get::<_, i32>(11)? != 0,
            supports_vision: row.get::<_, i32>(12)? != 0,
            capabilities: serde_json::from_str(
                row.get::<_, String>(13).unwrap_or_default().as_str(),
            )
            .unwrap_or(serde_json::Value::Array(vec![])),
            source_feed: row.get(14)?,
            is_active: row.get::<_, i32>(15)? != 0,
            last_synced: row.get(16)?,
            created_at: row.get(17)?,
            updated_at: row.get(18)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 按 provider 查询
#[allow(dead_code)]
pub fn list_by_provider(conn: &Connection, provider: &str) -> Result<Vec<CatalogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model, name, context_window, rpm, rpd, tpm, tpd,
         free_quota, supports_tools, supports_streaming, supports_vision, capabilities,
         source_feed, is_active, last_synced, created_at, updated_at
         FROM model_catalog WHERE provider = ?1 AND is_active = 1 ORDER BY model",
    )?;
    let rows = stmt.query_map(params![provider], |row| {
        Ok(CatalogEntry {
            id: row.get(0)?,
            provider: row.get(1)?,
            model: row.get(2)?,
            name: row.get(3)?,
            context_window: row.get(4)?,
            rpm: row.get(5)?,
            rpd: row.get(6)?,
            tpm: row.get(7)?,
            tpd: row.get(8)?,
            free_quota: row.get(9)?,
            supports_tools: row.get::<_, i32>(10)? != 0,
            supports_streaming: row.get::<_, i32>(11)? != 0,
            supports_vision: row.get::<_, i32>(12)? != 0,
            capabilities: serde_json::from_str(
                row.get::<_, String>(13).unwrap_or_default().as_str(),
            )
            .unwrap_or(serde_json::Value::Array(vec![])),
            source_feed: row.get(14)?,
            is_active: row.get::<_, i32>(15)? != 0,
            last_synced: row.get(16)?,
            created_at: row.get(17)?,
            updated_at: row.get(18)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 插入或更新目录条目（UPSERT）
pub fn upsert(conn: &Connection, entry: &CatalogEntry) -> Result<()> {
    let caps = serde_json::to_string(&entry.capabilities).unwrap_or_default();
    conn.execute(
        "INSERT INTO model_catalog (id, provider, model, name, context_window, rpm, rpd, tpm, tpd,
         free_quota, supports_tools, supports_streaming, supports_vision, capabilities,
         source_feed, is_active, last_synced, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
         ON CONFLICT(provider, model) DO UPDATE SET
            name = excluded.name, context_window = excluded.context_window,
            rpm = excluded.rpm, rpd = excluded.rpd, tpm = excluded.tpm, tpd = excluded.tpd,
            free_quota = excluded.free_quota, supports_tools = excluded.supports_tools,
            supports_streaming = excluded.supports_streaming, supports_vision = excluded.supports_vision,
            capabilities = excluded.capabilities, source_feed = excluded.source_feed,
            is_active = excluded.is_active, last_synced = excluded.last_synced, updated_at = excluded.updated_at",
        params![entry.id, entry.provider, entry.model, entry.name, entry.context_window,
         entry.rpm, entry.rpd, entry.tpm, entry.tpd, entry.free_quota,
         entry.supports_tools as i32, entry.supports_streaming as i32, entry.supports_vision as i32,
         caps, entry.source_feed, entry.is_active as i32, entry.last_synced, entry.created_at, entry.updated_at],
    )?;
    Ok(())
}

/// 批量 UPSERT
pub fn batch_upsert(conn: &Connection, entries: &[CatalogEntry]) -> Result<()> {
    for entry in entries {
        upsert(conn, entry)?;
    }
    Ok(())
}

/// 获取最后同步时间
#[allow(dead_code)]
pub fn last_sync_time(conn: &Connection) -> Result<Option<String>> {
    let result: Option<String> = conn
        .query_row(
            "SELECT MAX(last_synced) FROM model_catalog WHERE last_synced IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .ok();
    Ok(result)
}

/// 清理孤儿目录：删除 provider 不在保留集合中的记录。
///
/// 目录记录的 provider 归属与 catalog_sync 一致：连接的 customId（若有）
/// 或连接 provider 名——连接删除后其目录记录即成孤儿（sync 只 upsert
/// 不删）。`dry_run = true` 只统计不删除。返回按 provider 的删除计数
/// （按数量降序），保留集合为空时视作全部为孤儿（用户已删光连接）。
pub fn cleanup_orphans(
    conn: &Connection,
    keep: &[String],
    dry_run: bool,
) -> Result<Vec<(String, i64)>> {
    let not_in = if keep.is_empty() {
        "1=1".to_string()
    } else {
        let placeholders = std::iter::repeat_n("?", keep.len())
            .collect::<Vec<_>>()
            .join(",");
        format!("provider NOT IN ({placeholders})")
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT provider, COUNT(*) FROM model_catalog WHERE {not_in} GROUP BY provider ORDER BY COUNT(*) DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(keep.iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let stats: Vec<(String, i64)> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    if !dry_run && !stats.is_empty() {
        conn.execute(
            &format!("DELETE FROM model_catalog WHERE {not_in}"),
            rusqlite::params_from_iter(keep.iter()),
        )?;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&conn).unwrap();
        conn
    }

    fn add(conn: &Connection, provider: &str, model: &str) {
        upsert(
            conn,
            &CatalogEntry {
                id: format!("{provider}/{model}"),
                provider: provider.to_string(),
                model: model.to_string(),
                name: None,
                context_window: None,
                rpm: None,
                rpd: None,
                tpm: None,
                tpd: None,
                free_quota: None,
                supports_tools: false,
                supports_streaming: false,
                supports_vision: false,
                capabilities: serde_json::Value::Null,
                source_feed: None,
                is_active: true,
                last_synced: None,
                created_at: "2026-10-21".into(),
                updated_at: "2026-10-21".into(),
            },
        )
        .unwrap();
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM model_catalog", [], |r| r.get(0))
            .unwrap()
    }

    /// dry_run 只统计不删除；执行后仅删孤儿 provider 的记录
    #[test]
    fn cleanup_orphans_dry_run_and_execute() {
        let conn = setup();
        add(&conn, "agens", "m1");
        add(&conn, "agens", "m2");
        add(&conn, "tokenRouter", "m3");
        add(&conn, "claude", "m4");

        // 预览：两个孤儿 provider 共 2 条
        let preview = cleanup_orphans(&conn, &["agens".into()], true).unwrap();
        assert_eq!(preview.len(), 2);
        assert_eq!(preview.iter().map(|(_, n)| n).sum::<i64>(), 2);

        // 执行：删除后仅剩 agens 的 2 条
        let stats = cleanup_orphans(&conn, &["agens".into()], false).unwrap();
        assert_eq!(stats.iter().map(|(_, n)| n).sum::<i64>(), 2);
        assert_eq!(count(&conn), 2);
        assert!(list_by_provider(&conn, "agens").unwrap().len() == 2);
    }

    /// 保留集合为空（连接已删光）：全部目录视为孤儿，NOT IN 空集不炸 SQL
    #[test]
    fn cleanup_orphans_empty_keep_set() {
        let conn = setup();
        add(&conn, "agens", "m1");
        add(&conn, "claude", "m2");
        let stats = cleanup_orphans(&conn, &[], false).unwrap();
        assert_eq!(stats.iter().map(|(_, n)| n).sum::<i64>(), 2);
        assert_eq!(count(&conn), 0);
    }

    /// 无孤儿时执行删除为 no-op，统计为空
    #[test]
    fn cleanup_orphans_noop_when_clean() {
        let conn = setup();
        add(&conn, "agens", "m1");
        let stats = cleanup_orphans(&conn, &["agens".into()], false).unwrap();
        assert!(stats.is_empty());
        assert_eq!(count(&conn), 1);
    }
}
