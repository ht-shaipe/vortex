use rusqlite::{params, Row};
use crate::db::models::ProviderConnection;
use crate::error::Result;

fn encrypt_value(key: &[u8], plaintext: &str) -> Option<String> {
    if plaintext.is_empty() { return None; }
    match crate::db::encryption::encrypt(key, plaintext) {
        Ok(cipher) => Some(format!("enc:{}", cipher)),
        Err(_) => Some(plaintext.to_string()),
    }
}

fn decrypt_value(key: &[u8], value: &str) -> Option<String> {
    if value.is_empty() { return None; }
    if let Some(cipher) = value.strip_prefix("enc:") {
        match crate::db::encryption::decrypt(key, cipher) {
            Ok(plain) => Some(plain),
            Err(_) => Some(value.to_string()),
        }
    } else {
        Some(value.to_string())
    }
}

pub fn list(conn: &rusqlite::Connection, enc_key: &[u8]) -> Result<Vec<ProviderConnection>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, auth_type, name, email, priority, is_active, \
         access_token, refresh_token, expires_at, api_key, id_token, project_id, \
         test_status, error_code, last_error, last_error_at, backoff_level, \
         rate_limited_until, health_check_interval, consecutive_use_count, \
         rate_limit_protection, group_name, max_concurrent, proxy_enabled, \
         display_name, default_model, token_type, scope, last_used_at, \
         last_health_check_at, last_tested, provider_specific_data, \
         created_at, updated_at \
         FROM provider_connections ORDER BY priority DESC, name ASC"
    )?;

    let rows = stmt.query_map([], |row| row_to_connection(row, enc_key))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn list_by_provider(conn: &rusqlite::Connection, provider: &str, enc_key: &[u8]) -> Result<Vec<ProviderConnection>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, auth_type, name, email, priority, is_active, \
         access_token, refresh_token, expires_at, api_key, id_token, project_id, \
         test_status, error_code, last_error, last_error_at, backoff_level, \
         rate_limited_until, health_check_interval, consecutive_use_count, \
         rate_limit_protection, group_name, max_concurrent, proxy_enabled, \
         display_name, default_model, token_type, scope, last_used_at, \
         last_health_check_at, last_tested, provider_specific_data, \
         created_at, updated_at \
         FROM provider_connections WHERE provider = ?1 AND is_active = 1 \
         ORDER BY priority DESC, name ASC"
    )?;

    let rows = stmt.query_map(params![provider], |row| row_to_connection(row, enc_key))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_by_id(conn: &rusqlite::Connection, id: &str, enc_key: &[u8]) -> Result<Option<ProviderConnection>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, auth_type, name, email, priority, is_active, \
         access_token, refresh_token, expires_at, api_key, id_token, project_id, \
         test_status, error_code, last_error, last_error_at, backoff_level, \
         rate_limited_until, health_check_interval, consecutive_use_count, \
         rate_limit_protection, group_name, max_concurrent, proxy_enabled, \
         display_name, default_model, token_type, scope, last_used_at, \
         last_health_check_at, last_tested, provider_specific_data, \
         created_at, updated_at \
         FROM provider_connections WHERE id = ?1"
    )?;

    let mut rows = stmt.query_map(params![id], |row| row_to_connection(row, enc_key))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn create(conn: &rusqlite::Connection, req: &crate::db::models::CreateProviderRequest, enc_key: &[u8]) -> Result<ProviderConnection> {
    let id = uuid::Uuid::new_v4().to_string();
    let auth_type = req.auth_type.as_deref().unwrap_or("apikey");
    let now = chrono::Utc::now().to_rfc3339();

    // 组装 provider_specific_data：合并三处来源（请求里已有 / baseUrl / displayName / apiProtocol / customId）
    let mut specific_data = req.provider_specific_data.clone().unwrap_or(serde_json::json!({}));
    if !specific_data.is_object() {
        specific_data = serde_json::json!({});
    }
    let obj = specific_data.as_object_mut().unwrap();
    if let Some(base_url) = req.base_url.as_deref() {
        obj.insert("baseUrl".to_string(), serde_json::Value::String(base_url.to_string()));
    }
    if let Some(display_name) = req.display_name.as_deref() {
        obj.insert("displayName".to_string(), serde_json::Value::String(display_name.to_string()));
    }
    if let Some(api_protocol) = req.api_protocol.as_deref() {
        obj.insert("apiProtocol".to_string(), serde_json::Value::String(api_protocol.to_string()));
    }
    if let Some(custom_id) = req.custom_provider_id.as_deref() {
        obj.insert("customId".to_string(), serde_json::Value::String(custom_id.to_string()));
    }

    let encrypted_api_key = req.api_key.as_deref().and_then(|k| encrypt_value(enc_key, k));
    let encrypted_access_token = req.access_token.as_deref().and_then(|k| encrypt_value(enc_key, k));
    let encrypted_refresh_token = req.refresh_token.as_deref().and_then(|k| encrypt_value(enc_key, k));

    conn.execute(
        "INSERT INTO provider_connections \
         (id, provider, auth_type, name, email, priority, is_active, api_key, \
         access_token, refresh_token, project_id, rate_limit_protection, \
         group_name, max_concurrent, default_model, display_name, provider_specific_data, \
         test_status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?13, ?14, ?15, 'unknown', ?16, ?17)",
        params![
            id, req.provider, auth_type, req.name, req.email, req.priority.unwrap_or(0),
            encrypted_api_key, encrypted_access_token, encrypted_refresh_token, req.project_id,
            req.group_name, req.max_concurrent, req.default_model,
            req.display_name.clone().unwrap_or_default(),
            specific_data.to_string(), now, now
        ],
    )?;

    Ok(ProviderConnection {
        id,
        provider: req.provider.clone(),
        auth_type: auth_type.to_string(),
        name: req.name.clone(),
        email: req.email.clone(),
        priority: req.priority.unwrap_or(0),
        is_active: true,
        access_token: req.access_token.clone(),
        refresh_token: req.refresh_token.clone(),
        expires_at: None,
        api_key: req.api_key.clone(),
        id_token: None,
        project_id: req.project_id.clone(),
        test_status: "unknown".to_string(),
        error_code: None,
        last_error: None,
        last_error_at: None,
        backoff_level: 0,
        rate_limited_until: None,
        health_check_interval: 300,
        consecutive_use_count: 0,
        rate_limit_protection: false,
        group_name: req.group_name.clone(),
        max_concurrent: req.max_concurrent,
        proxy_enabled: false,
        display_name: req.display_name.clone(),
        default_model: req.default_model.clone(),
        token_type: None,
        scope: None,
        last_used_at: None,
        last_health_check_at: None,
        last_tested: None,
        provider_specific_data: specific_data,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update(conn: &rusqlite::Connection, id: &str, updates: &serde_json::Value, enc_key: &[u8]) -> Result<Option<ProviderConnection>> {
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id])?;
    }
    if let Some(api_key) = updates.get("apiKey").and_then(|v| v.as_str()) {
        let encrypted = encrypt_value(enc_key, api_key);
        conn.execute("UPDATE provider_connections SET api_key = ?1, updated_at = ?2 WHERE id = ?3", params![encrypted, now, id])?;
    }
    if let Some(is_active) = updates.get("isActive").and_then(|v| v.as_bool()) {
        conn.execute("UPDATE provider_connections SET is_active = ?1, updated_at = ?2 WHERE id = ?3", params![is_active as i32, now, id])?;
    }
    if let Some(priority) = updates.get("priority").and_then(|v| v.as_i64()) {
        conn.execute("UPDATE provider_connections SET priority = ?1, updated_at = ?2 WHERE id = ?3", params![priority as i32, now, id])?;
    }
    if let Some(default_model) = updates.get("defaultModel").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET default_model = ?1, updated_at = ?2 WHERE id = ?3", params![default_model, now, id])?;
    }
    if let Some(base_url) = updates.get("baseUrl").and_then(|v| v.as_str()) {
        let existing = get_by_id(conn, id, enc_key)?;
        if let Some(mut p) = existing {
            let mut data = p.provider_specific_data.as_object_mut()
                .cloned()
                .unwrap_or_default();
            data.insert("baseUrl".to_string(), serde_json::Value::String(base_url.to_string()));
            let data_str = serde_json::to_string(&data)?;
            conn.execute("UPDATE provider_connections SET provider_specific_data = ?1, updated_at = ?2 WHERE id = ?3", params![data_str, now, id])?;
        }
    }
    // displayName / apiProtocol：列与 JSON 双写。列给列表展示，JSON 给代理层读取。
    let mut dirty_extra: Option<serde_json::Value> = None;
    if let Some(dn) = updates.get("displayName").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET display_name = ?1, updated_at = ?2 WHERE id = ?3", params![dn, now, id])?;
    }
    if updates.get("displayName").map(|v| v.is_null()).unwrap_or(false) {
        conn.execute("UPDATE provider_connections SET display_name = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    if let Some(ap) = updates.get("apiProtocol").and_then(|v| v.as_str()) {
        let existing = get_by_id(conn, id, enc_key)?;
        if let Some(mut p) = existing {
            let mut data = p.provider_specific_data.as_object_mut().cloned().unwrap_or_default();
            data.insert("apiProtocol".to_string(), serde_json::Value::String(ap.to_string()));
            dirty_extra = Some(serde_json::Value::Object(data));
        }
    }
    if let Some(data) = dirty_extra {
        conn.execute("UPDATE provider_connections SET provider_specific_data = ?1, updated_at = ?2 WHERE id = ?3", params![data.to_string(), now, id])?;
    }

    get_by_id(conn, id, enc_key)
}

pub fn delete(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM provider_connections WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

pub fn update_test_status(conn: &rusqlite::Connection, id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET test_status = ?1, last_error = ?2, last_tested = ?3, updated_at = ?4 WHERE id = ?5",
        params![status, error, now, now, id],
    )?;
    Ok(())
}

pub fn increment_usage(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET consecutive_use_count = consecutive_use_count + 1, last_used_at = ?1, updated_at = ?2 WHERE id = ?3",
        params![now, now, id],
    )?;
    Ok(())
}

pub fn reset_backoff(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET backoff_level = 0, rate_limited_until = NULL, consecutive_use_count = 0, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

pub fn increment_backoff(conn: &rusqlite::Connection, id: &str, rate_limited_until: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET backoff_level = backoff_level + 1, rate_limited_until = ?1, updated_at = ?2 WHERE id = ?3",
        params![rate_limited_until, now, id],
    )?;
    Ok(())
}

fn row_to_connection(row: &Row, enc_key: &[u8]) -> rusqlite::Result<ProviderConnection> {
    let specific_data_str: String = row.get(32)?;
    let specific_data = serde_json::from_str(&specific_data_str).unwrap_or(serde_json::json!({}));

    let api_key_raw: Option<String> = row.get(10)?;
    let api_key = api_key_raw.as_deref().and_then(|v| decrypt_value(enc_key, v));

    let access_token_raw: Option<String> = row.get(7)?;
    let access_token = access_token_raw.as_deref().and_then(|v| decrypt_value(enc_key, v));

    let refresh_token_raw: Option<String> = row.get(8)?;
    let refresh_token = refresh_token_raw.as_deref().and_then(|v| decrypt_value(enc_key, v));

    Ok(ProviderConnection {
        id: row.get(0)?,
        provider: row.get(1)?,
        auth_type: row.get(2)?,
        name: row.get(3)?,
        email: row.get(4)?,
        priority: row.get(5)?,
        is_active: row.get::<_, i32>(6)? != 0,
        access_token,
        refresh_token,
        expires_at: row.get(9)?,
        api_key,
        id_token: row.get(11)?,
        project_id: row.get(12)?,
        test_status: row.get(13)?,
        error_code: row.get(14)?,
        last_error: row.get(15)?,
        last_error_at: row.get(16)?,
        backoff_level: row.get(17)?,
        rate_limited_until: row.get(18)?,
        health_check_interval: row.get(19)?,
        consecutive_use_count: row.get(20)?,
        rate_limit_protection: row.get::<_, i32>(21)? != 0,
        group_name: row.get(22)?,
        max_concurrent: row.get(23)?,
        proxy_enabled: row.get::<_, i32>(24)? != 0,
        display_name: row.get(25)?,
        default_model: row.get(26)?,
        token_type: row.get(27)?,
        scope: row.get(28)?,
        last_used_at: row.get(29)?,
        last_health_check_at: row.get(30)?,
        last_tested: row.get(31)?,
        provider_specific_data: specific_data,
        created_at: row.get(33)?,
        updated_at: row.get(34)?,
    })
}
