//! API Key 级别的模型访问控制权限。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 权限规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyPermission {
    pub id: String,
    pub api_key_id: String,
    pub rule_type: String,
    pub model_pattern: String,
    pub created_at: String,
}

/// 创建权限规则
pub fn create(conn: &Connection, api_key_id: &str, rule_type: &str, model_pattern: &str) -> Result<KeyPermission> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO key_permissions (id, api_key_id, rule_type, model_pattern, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, api_key_id, rule_type, model_pattern, now],
    )?;
    Ok(KeyPermission { id, api_key_id: api_key_id.to_string(), rule_type: rule_type.to_string(), model_pattern: model_pattern.to_string(), created_at: now })
}

/// 列出某 API Key 的所有权限规则
pub fn list_by_key(conn: &Connection, api_key_id: &str) -> Result<Vec<KeyPermission>> {
    let mut stmt = conn.prepare(
        "SELECT id, api_key_id, rule_type, model_pattern, created_at FROM key_permissions WHERE api_key_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![api_key_id], |row| {
        Ok(KeyPermission { id: row.get(0)?, api_key_id: row.get(1)?, rule_type: row.get(2)?, model_pattern: row.get(3)?, created_at: row.get(4)? })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 列出所有权限规则
pub fn list_all(conn: &Connection) -> Result<Vec<KeyPermission>> {
    let mut stmt = conn.prepare(
        "SELECT id, api_key_id, rule_type, model_pattern, created_at FROM key_permissions ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(KeyPermission { id: row.get(0)?, api_key_id: row.get(1)?, rule_type: row.get(2)?, model_pattern: row.get(3)?, created_at: row.get(4)? })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 删除权限规则
pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM key_permissions WHERE id = ?1", params![id])?;
    Ok(())
}

/// 检查模型是否被允许访问
pub fn check_access(conn: &Connection, api_key_id: &str, model: &str) -> Result<bool> {
    let rules = list_by_key(conn, api_key_id)?;
    if rules.is_empty() {
        return Ok(true);
    }
    for rule in &rules {
        let pattern = &rule.model_pattern;
        let matched = if pattern == "*" {
            true
        } else if pattern.contains('*') {
            let prefix = pattern.split('*').next().unwrap_or("");
            model.starts_with(prefix)
        } else {
            model == pattern
        };
        if matched {
            return Ok(rule.rule_type == "allow");
        }
    }
    Ok(true)
}
