use rusqlite::{params};
use crate::error::Result;

pub fn get(conn: &rusqlite::Connection, namespace: &str, key: &str) -> Result<Option<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT value FROM key_value WHERE namespace = ?1 AND key = ?2"
    )?;
    let mut rows = stmt.query_map(params![namespace, key], |row| {
        let v: String = row.get(0)?;
        Ok(v)
    })?;

    match rows.next() {
        Some(Ok(v)) => Ok(Some(serde_json::from_str(&v).unwrap_or(serde_json::json!({})))),
        _ => Ok(None),
    }
}

pub fn set(conn: &rusqlite::Connection, namespace: &str, key: &str, value: &serde_json::Value) -> Result<()> {
    let value_str = serde_json::to_string(value)?;
    conn.execute(
        "INSERT INTO key_value (namespace, key, value) VALUES (?1, ?2, ?3) \
         ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value",
        params![namespace, key, value_str],
    )?;
    Ok(())
}

pub fn get_settings(conn: &rusqlite::Connection) -> Result<serde_json::Value> {
    let general = get(conn, "settings", "general")?.unwrap_or(serde_json::json!({}));
    let routing = get(conn, "settings", "routing")?.unwrap_or(serde_json::json!({}));
    Ok(serde_json::json!({
        "general": general,
        "routing": routing,
    }))
}

pub fn update_settings(conn: &rusqlite::Connection, updates: &serde_json::Value) -> Result<serde_json::Value> {
    if let Some(general) = updates.get("general") {
        set(conn, "settings", "general", general)?;
    }
    if let Some(routing) = updates.get("routing") {
        set(conn, "settings", "routing", routing)?;
    }
    get_settings(conn)
}
