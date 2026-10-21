//! 路由配置文件 CRUD（命名回退链）。
//!
//! 一个路由配置包含有序的目标列表（provider + model + 可选 connection_id）；
//! 请求按目标顺序尝试，失败时回退到下一个目标。

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 路由配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingProfile {
    pub id: String,
    /// 配置名称（请求中通过该名称引用）
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 有序回退目标列表
    pub targets: Vec<ProfileTarget>,
    /// 是否启用；仅启用的配置参与解析
    #[serde(default)]
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 单个回退目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileTarget {
    /// 提供商 ID
    pub provider: String,
    /// 模型名
    pub model: String,
    /// 指定连接 ID（可选；缺省时由路由层在 provider 下选择连接）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

/// 创建路由配置请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutingProfile {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub targets: Vec<ProfileTarget>,
    /// 缺省为启用
    #[serde(default)]
    pub is_active: Option<bool>,
}

/// 列出全部路由配置，按名称排序。
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

/// 按名称查询启用的路由配置；未启用或不存在时返回 `None`。
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

/// 创建路由配置并返回落库后的完整记录（`is_active` 缺省为 true）。
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

/// 按 ID 删除路由配置（不存在时静默成功）。
pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM routing_profiles WHERE id = ?1", params![id])?;
    Ok(())
}
