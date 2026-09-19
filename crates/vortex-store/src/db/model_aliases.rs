//! 模型别名（虚拟模型映射）CRUD 操作。

use crate::db::models::{CreateModelAlias, ModelAlias, ModelAliasTarget, UpdateModelAlias};
use rusqlite::{params, Connection};
use uuid::Uuid;

/// 行映射：从 DB 行构建 ModelAlias（含 source、sort_order 列）。
fn row_to_alias(row: &rusqlite::Row) -> rusqlite::Result<ModelAlias> {
    let targets_json: String = row.get(2)?;
    let targets: Vec<ModelAliasTarget> = serde_json::from_str(&targets_json).unwrap_or_default();
    Ok(ModelAlias {
        id: row.get(0)?,
        alias: row.get(1)?,
        targets,
        is_active: row.get::<_, i64>(3)? != 0,
        source: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        sort_order: row.get::<_, i64>(7).unwrap_or(0),
    })
}

const SELECT_COLS: &str = "id, alias, targets, is_active, source, created_at, updated_at, sort_order";

/// 列出所有模型别名，按 sort_order DESC、alias ASC 排序。
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<ModelAlias>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM model_aliases ORDER BY sort_order DESC, alias ASC"
    ))?;
    let rows = stmt.query_map([], row_to_alias)?;
    rows.collect()
}

/// 按 alias 查找模型别名。
pub fn get_by_alias(conn: &Connection, alias: &str) -> rusqlite::Result<Option<ModelAlias>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM model_aliases WHERE alias = ?1 AND is_active = 1"
    ))?;
    let mut rows = stmt.query_map(params![alias], row_to_alias)?;
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
        "INSERT INTO model_aliases (id, alias, targets, is_active, source, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'manual', ?5, ?6)",
        params![id, req.alias, targets_json, req.is_active as i64, now, now],
    )?;
    Ok(ModelAlias {
        id,
        alias: req.alias.clone(),
        targets: req.targets.clone(),
        is_active: req.is_active,
        source: "manual".into(),
        created_at: now.clone(),
        updated_at: now,
        sort_order: 0,
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
    if let Some(sort_order) = req.sort_order {
        conn.execute("UPDATE model_aliases SET sort_order = ?1, updated_at = ?2 WHERE id = ?3", params![sort_order, now, id])?;
    }
    let mut stmt = conn.prepare(&format!("SELECT {SELECT_COLS} FROM model_aliases WHERE id = ?1"))?;
    let mut rows = stmt.query_map(params![id], row_to_alias)?;
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

/// 批量更新排序权重。接受 (id, sort_order) 列表，逐条更新。
pub fn batch_reorder(conn: &Connection, orders: &[(String, i64)]) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    for (id, sort_order) in orders {
        conn.execute(
            "UPDATE model_aliases SET sort_order = ?1, updated_at = ?2 WHERE id = ?3",
            params![sort_order, now, id],
        )?;
    }
    Ok(())
}

// ── 自动归纳操作 ──

/// 删除所有 source='auto' 的别名（重新归纳前调用）。
pub fn delete_all_auto(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM model_aliases WHERE source = 'auto'", [])?;
    Ok(())
}

/// 批量创建自动归纳的别名。跳过与现有 manual 别名同名的条目。
pub fn create_auto_batch(
    conn: &Connection,
    aliases: &[(String, Vec<ModelAliasTarget>)],
) -> rusqlite::Result<Vec<ModelAlias>> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut created = Vec::new();

    // 查询已有 manual 别名集合，避免覆盖
    let mut stmt = conn.prepare("SELECT alias FROM model_aliases WHERE source = 'manual'")?;
    let manual_aliases: std::collections::HashSet<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    for (alias, targets) in aliases {
        if manual_aliases.contains(alias) {
            continue;
        }
        let id = Uuid::new_v4().to_string();
        let targets_json = serde_json::to_string(targets).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO model_aliases (id, alias, targets, is_active, source, created_at, updated_at) VALUES (?1, ?2, ?3, 1, 'auto', ?4, ?5)",
            params![id, alias, targets_json, now, now],
        )?;
        created.push(ModelAlias {
            id,
            alias: alias.clone(),
            targets: targets.clone(),
            is_active: true,
            source: "auto".into(),
            created_at: now.clone(),
            updated_at: now.clone(),
            sort_order: 0,
        });
    }

    Ok(created)
}
