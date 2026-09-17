//! 内容寻址回忆：存储和检索被压缩前的原始 prompt 内容。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 压缩内容记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressedContent {
    pub hash: String,
    #[serde(skip_serializing)]
    pub original_content: String,
    pub content_size: i64,
    pub saved_tokens: i64,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

/// 计算内容的 SHA-256 hash
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 存储被压缩的原始内容
pub fn store(
    conn: &Connection,
    hash: &str,
    original_content: &str,
    saved_tokens: i64,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let expires = chrono::Utc::now() + chrono::Duration::days(30);
    conn.execute(
        "INSERT OR REPLACE INTO compressed_content \
         (hash, original_content, content_size, saved_tokens, provider, model, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            hash,
            original_content,
            original_content.len() as i64,
            saved_tokens,
            provider,
            model,
            now,
            expires.to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// 按 hash 检索原始内容
pub fn retrieve(conn: &Connection, hash: &str) -> Result<Option<CompressedContent>> {
    let mut stmt = conn.prepare(
        "SELECT hash, original_content, content_size, saved_tokens, provider, model, created_at, expires_at \
         FROM compressed_content WHERE hash = ?1",
    )?;
    let mut rows = stmt.query_map(params![hash], |row| {
        Ok(CompressedContent {
            hash: row.get(0)?,
            original_content: row.get(1)?,
            content_size: row.get(2)?,
            saved_tokens: row.get(3)?,
            provider: row.get(4)?,
            model: row.get(5)?,
            created_at: row.get(6)?,
            expires_at: row.get(7)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}

/// 列出最近的压缩内容记录（不包含 original_content 字段）
pub fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<CompressedContent>> {
    let mut stmt = conn.prepare(
        "SELECT hash, '', content_size, saved_tokens, provider, model, created_at, expires_at \
         FROM compressed_content ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(CompressedContent {
            hash: row.get(0)?,
            original_content: String::new(),
            content_size: row.get(2)?,
            saved_tokens: row.get(3)?,
            provider: row.get(4)?,
            model: row.get(5)?,
            created_at: row.get(6)?,
            expires_at: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 清理过期记录
pub fn cleanup_expired(conn: &Connection) -> Result<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let n = conn.execute(
        "DELETE FROM compressed_content WHERE expires_at IS NOT NULL AND expires_at < ?1",
        params![now],
    )?;
    Ok(n)
}
