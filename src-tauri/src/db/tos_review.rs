//! ToS 合规审查 CRUD。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TosReview {
    pub id: String,
    pub provider: String,
    pub verdict: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

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
