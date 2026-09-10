use rusqlite::{params, Row};
use crate::db::models::{Combo, ComboData, CreateComboRequest};
use crate::error::Result;

pub fn list(conn: &rusqlite::Connection) -> Result<Vec<Combo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, data, sort_order, context_cache_protection, created_at, updated_at \
         FROM combos ORDER BY sort_order ASC, name ASC"
    )?;
    let rows = stmt.query_map([], |row| row_to_combo(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_by_id(conn: &rusqlite::Connection, id: &str) -> Result<Option<Combo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, data, sort_order, context_cache_protection, created_at, updated_at \
         FROM combos WHERE id = ?1"
    )?;
    let mut rows = stmt.query_map(params![id], |row| row_to_combo(row))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn get_by_name(conn: &rusqlite::Connection, name: &str) -> Result<Option<Combo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, data, sort_order, context_cache_protection, created_at, updated_at \
         FROM combos WHERE name = ?1"
    )?;
    let mut rows = stmt.query_map(params![name], |row| row_to_combo(row))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn create(conn: &rusqlite::Connection, req: &CreateComboRequest) -> Result<Combo> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let data = ComboData {
        strategy: req.strategy.clone().unwrap_or_else(|| "priority".to_string()),
        models: req.models.clone().unwrap_or_default(),
        config: req.config.clone().unwrap_or(serde_json::json!({})),
        system_message: req.system_message.clone(),
    };

    let data_json = serde_json::to_string(&data)?;

    let max_sort: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) FROM combos", [], |row| row.get(0)
    ).unwrap_or(0);

    conn.execute(
        "INSERT INTO combos (id, name, data, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, req.name, data_json, max_sort + 1, now, now],
    )?;

    Ok(Combo {
        id,
        name: req.name.clone(),
        data,
        sort_order: max_sort + 1,
        context_cache_protection: false,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update(conn: &rusqlite::Connection, id: &str, req: &serde_json::Value) -> Result<Option<Combo>> {
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(name) = req.get("name").and_then(|v| v.as_str()) {
        conn.execute("UPDATE combos SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id])?;
    }
    if let Some(data_val) = req.get("data") {
        let data_json = serde_json::to_string(data_val)?;
        conn.execute("UPDATE combos SET data = ?1, updated_at = ?2 WHERE id = ?3", params![data_json, now, id])?;
    }
    if let Some(strategy) = req.get("strategy").and_then(|v| v.as_str()) {
        let existing = get_by_id(conn, id)?;
        if let Some(mut combo) = existing {
            combo.data.strategy = strategy.to_string();
            let data_json = serde_json::to_string(&combo.data)?;
            conn.execute("UPDATE combos SET data = ?1, updated_at = ?2 WHERE id = ?3", params![data_json, now, id])?;
        }
    }
    if let Some(models) = req.get("models") {
        let existing = get_by_id(conn, id)?;
        if let Some(mut combo) = existing {
            if let Ok(parsed) = serde_json::from_value::<Vec<crate::db::models::ComboStep>>(models.clone()) {
                combo.data.models = parsed;
                let data_json = serde_json::to_string(&combo.data)?;
                conn.execute("UPDATE combos SET data = ?1, updated_at = ?2 WHERE id = ?3", params![data_json, now, id])?;
            }
        }
    }

    get_by_id(conn, id)
}

pub fn delete(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM combos WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

fn row_to_combo(row: &Row) -> rusqlite::Result<Combo> {
    let data_str: String = row.get(2)?;
    let data: ComboData = serde_json::from_str(&data_str).unwrap_or(ComboData {
        strategy: "priority".to_string(),
        models: vec![],
        config: serde_json::json!({}),
        system_message: None,
    });

    Ok(Combo {
        id: row.get(0)?,
        name: row.get(1)?,
        data,
        sort_order: row.get(3)?,
        context_cache_protection: row.get::<_, i32>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
