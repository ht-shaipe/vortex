//! 路由配置文件 CRUD（命名回退链）。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub targets: Vec<ProfileTarget>,
    #[serde(default)]
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileTarget {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutingProfile {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub targets: Vec<ProfileTarget>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

pub fn list(conn: &Connection) -> Result<Vec<RoutingProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, targets, is_active, created_at, updated_at FROM routing_profiles ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        let targets_str: String = row.get(3)?;
        Ok(RoutingProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            targets: serde_json::from_str(&targets_str).unwrap_or_default(),
            is_active: row.get::<_, i32>(4)? != 0,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get_by_name(conn: &Connection, name: &str) -> Result<Option<RoutingProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, targets, is_active, created_at, updated_at FROM routing_profiles WHERE name = ?1 AND is_active = 1",
    )?;
    let mut rows = stmt.query_map(params![name], |row| {
        let targets_str: String = row.get(3)?;
        Ok(RoutingProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            targets: serde_json::from_str(&targets_str).unwrap_or_default(),
            is_active: row.get::<_, i32>(4)? != 0,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}

pub fn create(conn: &Connection, req: &CreateRoutingProfile) -> Result<RoutingProfile> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let targets_str = serde_json::to_string(&req.targets).unwrap_or_default();
    let is_active = req.is_active.unwrap_or(true);
    conn.execute(
        "INSERT INTO routing_profiles (id, name, description, targets, is_active, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, req.name, req.description, targets_str, is_active as i32, now],
    )?;
    get_by_name(conn, &req.name)?.ok_or_else(|| crate::error::AppError::Internal("Failed to create routing profile".into()))
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM routing_profiles WHERE id = ?1", params![id])?;
    Ok(())
}
