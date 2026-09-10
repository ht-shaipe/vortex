use rusqlite::{params, Row};
use crate::db::models::{ApiKey, CreateApiKeyRequest};
use crate::error::Result;

const SELECT_COLS: &str = "id, name, key, machine_id, allowed_models, \
     allowed_connections, allowed_endpoints, no_log, auto_resolve, \
     is_active, is_banned, rate_limits, usage_limits, created_at";

pub fn list(conn: &rusqlite::Connection) -> Result<Vec<ApiKey>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM api_keys ORDER BY created_at DESC",
        SELECT_COLS
    ))?;
    let rows = stmt.query_map([], |row| row_to_key(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_by_id(conn: &rusqlite::Connection, id: &str) -> Result<Option<ApiKey>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM api_keys WHERE id = ?1",
        SELECT_COLS
    ))?;
    let mut rows = stmt.query_map(params![id], |row| row_to_key(row))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn get_by_key(conn: &rusqlite::Connection, key: &str) -> Result<Option<ApiKey>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM api_keys WHERE key = ?1 AND is_active = 1 AND is_banned = 0",
        SELECT_COLS
    ))?;
    let mut rows = stmt.query_map(params![key], |row| row_to_key(row))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn create(conn: &rusqlite::Connection, req: &CreateApiKeyRequest) -> Result<ApiKey> {
    let id = uuid::Uuid::new_v4().to_string();
    let key = format!("vx-{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let now = chrono::Utc::now().to_rfc3339();

    let allowed_models = serde_json::to_string(&req.allowed_models.clone().unwrap_or(serde_json::json!([])))?;
    let rate_limits = serde_json::to_string(&req.rate_limits.clone().unwrap_or(serde_json::json!([])))?;
    let usage_limits = serde_json::to_string(&req.usage_limits.clone().unwrap_or(serde_json::json!({})))?;

    conn.execute(
        "INSERT INTO api_keys (id, name, key, allowed_models, \
         allowed_connections, allowed_endpoints, no_log, auto_resolve, \
         is_active, is_banned, rate_limits, usage_limits, created_at) \
         VALUES (?1, ?2, ?3, ?4, '[]', '[]', 0, 1, 1, 0, ?5, ?6, ?7)",
        params![id, req.name, key, allowed_models, rate_limits, usage_limits, now],
    )?;

    Ok(ApiKey {
        id,
        name: req.name.clone(),
        key,
        machine_id: None,
        allowed_models: req.allowed_models.clone().unwrap_or(serde_json::json!([])),
        allowed_connections: serde_json::json!([]),
        allowed_endpoints: serde_json::json!([]),
        no_log: false,
        auto_resolve: true,
        is_active: true,
        is_banned: false,
        rate_limits: req.rate_limits.clone().unwrap_or(serde_json::json!([])),
        usage_limits: req.usage_limits.clone().unwrap_or(serde_json::json!({})),
        created_at: now,
    })
}

pub fn delete(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM api_keys WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

fn row_to_key(row: &Row) -> rusqlite::Result<ApiKey> {
    let allowed_models_str: String = row.get(4)?;
    let allowed_conn_str: String = row.get(5)?;
    let allowed_endpoints_str: String = row.get(6)?;
    let rate_limits_str: String = row.get(11)?;
    let usage_limits_str: String = row.get(12)?;

    Ok(ApiKey {
        id: row.get(0)?,
        name: row.get(1)?,
        key: row.get(2)?,
        machine_id: row.get(3)?,
        allowed_models: serde_json::from_str(&allowed_models_str).unwrap_or(serde_json::json!([])),
        allowed_connections: serde_json::from_str(&allowed_conn_str).unwrap_or(serde_json::json!([])),
        allowed_endpoints: serde_json::from_str(&allowed_endpoints_str).unwrap_or(serde_json::json!([])),
        no_log: row.get::<_, i32>(7)? != 0,
        auto_resolve: row.get::<_, i32>(8)? != 0,
        is_active: row.get::<_, i32>(9)? != 0,
        is_banned: row.get::<_, i32>(10)? != 0,
        rate_limits: serde_json::from_str(&rate_limits_str).unwrap_or(serde_json::json!([])),
        usage_limits: serde_json::from_str(&usage_limits_str).unwrap_or(serde_json::json!({})),
        created_at: row.get(13)?,
    })
}
