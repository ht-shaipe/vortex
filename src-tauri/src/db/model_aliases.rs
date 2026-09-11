//! 模型别名（虚拟模型映射）CRUD 操作。

use crate::db::models::{CreateModelAlias, ModelAlias, UpdateModelAlias};
use rusqlite::{params, Connection};
use uuid::Uuid;

/// 列出所有模型别名。
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<ModelAlias>> {
    let mut stmt = conn.prepare(
        "SELECT id, alias, targets, is_active, created_at, updated_at FROM model_aliases ORDER BY alias ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let targets_json: String = row.get(2)?;
        let targets: Vec<crate::db::models::ModelAliasTarget> =
            serde_json::from_str(&targets_json).unwrap_or_default();
        Ok(ModelAlias {
            id: row.get(0)?,
            alias: row.get(1)?,
            targets,
            is_active: row.get::<_, i64>(3)? != 0,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    rows.collect()
}

/// 按 alias 查找模型别名。
pub fn get_by_alias(conn: &Connection, alias: &str) -> rusqlite::Result<Option<ModelAlias>> {
    let mut stmt = conn.prepare(
        "SELECT id, alias, targets, is_active, created_at, updated_at FROM model_aliases WHERE alias = ?1 AND is_active = 1",
    )?;
    let mut rows = stmt.query_map(params![alias], |row| {
        let targets_json: String = row.get(2)?;
        let targets: Vec<crate::db::models::ModelAliasTarget> =
            serde_json::from_str(&targets_json).unwrap_or_default();
        Ok(ModelAlias {
            id: row.get(0)?,
            alias: row.get(1)?,
            targets,
            is_active: row.get::<_, i64>(3)? != 0,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

/// 创建模型别名。
pub fn create(conn: &Connection, req: &CreateModelAlias) -> rusqlite::Result<ModelAlias> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let targets_json = serde_json::to_string(&req.targets).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO model_aliases (id, alias, targets, is_active, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, req.alias, targets_json, req.is_active as i64, now, now],
    )?;
    Ok(ModelAlias {
        id,
        alias: req.alias.clone(),
        targets: req.targets.clone(),
        is_active: req.is_active,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// 更新模型别名。
pub fn update(conn: &Connection, id: &str, req: &UpdateModelAlias) -> rusqlite::Result<Option<ModelAlias>> {
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(alias) = &req.alias {
        conn.execute("UPDATE model_aliases SET alias = ?1, updated_at = ?2 WHERE id = ?3", params![alias, now, id])?;
    }
    if let Some(targets) = &req.targets {
        let targets_json = serde_json::to_string(targets).unwrap_or_else(|_| "[]".to_string());
        conn.execute("UPDATE model_aliases SET targets = ?1, updated_at = ?2 WHERE id = ?3", params![targets_json, now, id])?;
    }
    if let Some(is_active) = req.is_active {
        conn.execute("UPDATE model_aliases SET is_active = ?1, updated_at = ?2 WHERE id = ?3", params![is_active as i64, now, id])?;
    }
    let mut stmt = conn.prepare(
        "SELECT id, alias, targets, is_active, created_at, updated_at FROM model_aliases WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], |row| {
        let targets_json: String = row.get(2)?;
        let targets: Vec<crate::db::models::ModelAliasTarget> =
            serde_json::from_str(&targets_json).unwrap_or_default();
        Ok(ModelAlias {
            id: row.get(0)?,
            alias: row.get(1)?,
            targets,
            is_active: row.get::<_, i64>(3)? != 0,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    if let Some(row) = rows.next() {
        Ok(Some(row?))
    } else {
        Ok(None)
    }
}

/// 删除模型别名。
pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM model_aliases WHERE id = ?1", params![id])?;
    Ok(())
}
