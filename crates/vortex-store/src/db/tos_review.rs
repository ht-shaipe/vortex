//! ToS 合规审查 CRUD。
//!
//! 记录各提供商的服务条款审查结论（`verdict`）与备注，
//! 由管理端只读展示，数据通过迁移脚本预置。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 一条 ToS 审查记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TosReview {
    /// 记录 ID
    pub id: String,
    /// 提供商 ID（对应 provider registry 中的 id）
    pub provider: String,
    /// 审查结论（如 `allowed` / `restricted` / `forbidden`）
    pub verdict: String,
    /// 审查备注
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// 审查日期（ISO 日期字符串）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_date: Option<String>,
    /// 结论来源链接
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 列出全部审查记录，按提供商名称排序。
pub fn list_all(conn: &Connection) -> Result<Vec<TosReview>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, verdict, notes, review_date, source_url, created_at, updated_at FROM tos_review ORDER BY provider",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TosReview {
            id: row.get(0)?,
            provider: row.get(1)?,
            verdict: row.get(2)?,
            notes: row.get(3)?,
            review_date: row.get(4)?,
            source_url: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 按提供商查询审查记录（一个提供商至多一条）。
#[allow(dead_code)]
pub fn get_by_provider(conn: &Connection, provider: &str) -> Result<Option<TosReview>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, verdict, notes, review_date, source_url, created_at, updated_at FROM tos_review WHERE provider = ?1",
    )?;
    let mut rows = stmt.query_map(params![provider], |row| {
        Ok(TosReview {
            id: row.get(0)?,
            provider: row.get(1)?,
            verdict: row.get(2)?,
            notes: row.get(3)?,
            review_date: row.get(4)?,
            source_url: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}
