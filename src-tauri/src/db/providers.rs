//! 提供商连接 CRUD 操作模块。
//!
//! 负责对 `provider_connections` 表的增删改查，以及对敏感字段
//! （API Key、Access Token、Refresh Token）的加密存储与解密读取。
//! 更新操作支持部分字段更新，`provider_specific_data` JSON 列承载模型列表、
//! baseUrl、apiProtocol 等扩展信息。

use rusqlite::{params, Row};
use crate::db::models::{ProviderConnection, ProviderModel};
use crate::error::Result;

/// 加密单个敏感字段值。
///
/// 空字符串返回 `None`；加密成功则添加 `enc:` 前缀标记；
/// 加密失败时降级返回明文（保证可用性，不阻断流程）。
///
/// # 参数
/// - `key`：加密密钥
/// - `plaintext`：待加密明文
///
/// # 返回
/// 加密后的带前缀字符串（或降级明文），空输入返回 `None`
fn encrypt_value(key: &[u8], plaintext: &str) -> Option<String> {
    if plaintext.is_empty() { return None; }
    match crate::db::encryption::encrypt(key, plaintext) {
        Ok(cipher) => Some(format!("enc:{}", cipher)),
        Err(_) => Some(plaintext.to_string()),
    }
}

/// 解密单个敏感字段值。
///
/// 空字符串返回 `None`；带 `enc:` 前缀的值尝试解密；
/// 解密失败或无前缀时返回原值（兼容历史明文数据）。
///
/// # 参数
/// - `key`：解密密钥
/// - `value`：待解密的值（可能带 `enc:` 前缀）
///
/// # 返回
/// 解密后的明文或原值，空输入返回 `None`
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

/// 查询所有提供商连接列表。
///
/// 按优先级降序、名称升序排列，敏感字段在读取时自动解密。
///
/// # 参数
/// - `conn`：数据库连接
/// - `enc_key`：解密密钥
///
/// # 返回
/// 所有连接的列表
pub fn list(conn: &rusqlite::Connection, enc_key: &[u8]) -> Result<Vec<ProviderConnection>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, auth_type, name, email, priority, is_active, \
         access_token, refresh_token, expires_at, api_key, id_token, project_id, \
         test_status, error_code, last_error, last_error_at, backoff_level, \
         rate_limited_until, health_check_interval, consecutive_use_count, \
         rate_limit_protection, group_name, max_concurrent, proxy_enabled, \
         display_name, default_model, token_type, scope, last_used_at, \
         last_health_check_at, last_tested, provider_specific_data, \
         created_at, updated_at, last_latency_ms \
         FROM provider_connections ORDER BY priority DESC, name ASC"
    )?;

    let rows = stmt.query_map([], |row| row_to_connection(row, enc_key))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// 按提供商标识查询活跃连接列表。
///
/// 仅返回指定提供商且 `is_active = 1` 的连接，按优先级降序、名称升序排列。
///
/// # 参数
/// - `conn`：数据库连接
/// - `provider`：提供商标识
/// - `enc_key`：解密密钥
///
/// # 返回
/// 匹配的活跃连接列表
pub fn list_by_provider(conn: &rusqlite::Connection, provider: &str, enc_key: &[u8]) -> Result<Vec<ProviderConnection>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, auth_type, name, email, priority, is_active, \
         access_token, refresh_token, expires_at, api_key, id_token, project_id, \
         test_status, error_code, last_error, last_error_at, backoff_level, \
         rate_limited_until, health_check_interval, consecutive_use_count, \
         rate_limit_protection, group_name, max_concurrent, proxy_enabled, \
         display_name, default_model, token_type, scope, last_used_at, \
         last_health_check_at, last_tested, provider_specific_data, \
         created_at, updated_at, last_latency_ms \
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

/// 按 ID 查询单个提供商连接。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
/// - `enc_key`：解密密钥
///
/// # 返回
/// 找到返回 `Some(连接)`，未找到返回 `None`
pub fn get_by_id(conn: &rusqlite::Connection, id: &str, enc_key: &[u8]) -> Result<Option<ProviderConnection>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, auth_type, name, email, priority, is_active, \
         access_token, refresh_token, expires_at, api_key, id_token, project_id, \
         test_status, error_code, last_error, last_error_at, backoff_level, \
         rate_limited_until, health_check_interval, consecutive_use_count, \
         rate_limit_protection, group_name, max_concurrent, proxy_enabled, \
         display_name, default_model, token_type, scope, last_used_at, \
         last_health_check_at, last_tested, provider_specific_data, \
         created_at, updated_at, last_latency_ms \
         FROM provider_connections WHERE id = ?1"
    )?;

    let mut rows = stmt.query_map(params![id], |row| row_to_connection(row, enc_key))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// 创建新的提供商连接。
///
/// 生成 UUID 作为主键，组装 `provider_specific_data` JSON（合并请求中已有数据、
/// baseUrl、displayName、apiProtocol、customId、models），加密敏感字段后插入数据库。
/// 默认模型优先取请求显式指定值，否则取模型列表首个。
///
/// # 参数
/// - `conn`：数据库连接
/// - `req`：创建请求参数
/// - `enc_key`：加密密钥
///
/// # 返回
/// 创建成功的连接实体（含明文敏感字段）
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
    if let Some(models) = req.models.as_ref() {
        obj.insert(
            "models".to_string(),
            serde_json::to_value(models).unwrap_or(serde_json::Value::Null),
        );
    }

    // 加密敏感字段
    let encrypted_api_key = req.api_key.as_deref().and_then(|k| encrypt_value(enc_key, k));
    let encrypted_access_token = req.access_token.as_deref().and_then(|k| encrypt_value(enc_key, k));
    let encrypted_refresh_token = req.refresh_token.as_deref().and_then(|k| encrypt_value(enc_key, k));

    // 默认模型：请求显式指定优先；否则取模型列表首个作为默认（维持路由回退行为）。
    let default_model = req.default_model.clone().or_else(|| {
        req.models.as_ref().and_then(|m| m.first().map(|x| x.id.clone()))
    });

    // 插入数据库记录
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
            req.group_name, req.max_concurrent, default_model,
            req.display_name.clone().unwrap_or_default(),
            specific_data.to_string(), now, now
        ],
    )?;

    // 返回创建的连接实体（明文敏感字段）
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
        default_model: default_model.clone(),
        models: req.models.clone(),
        token_type: None,
        scope: None,
        last_used_at: None,
        last_health_check_at: None,
        last_tested: None,
        provider_specific_data: specific_data,
        created_at: now.clone(),
        updated_at: now,
        last_latency_ms: None,
    })
}

/// 部分更新提供商连接。
///
/// 接受 JSON 对象，仅更新出现的字段。支持 name、apiKey、isActive、priority、
/// defaultModel、email、projectId、groupName、maxConcurrent、rateLimitProtection、
/// proxyEnabled、healthCheckInterval、baseUrl、displayName、apiProtocol、models 等。
/// null 值用于清除对应字段。更新后返回最新连接。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
/// - `updates`：更新字段 JSON（camelCase 键名）
/// - `enc_key`：加密密钥
///
/// # 返回
/// 更新后的连接实体（找不到则返回 `None`）
pub fn update(conn: &rusqlite::Connection, id: &str, updates: &serde_json::Value, enc_key: &[u8]) -> Result<Option<ProviderConnection>> {
    let now = chrono::Utc::now().to_rfc3339();

    // 更新 name
    if let Some(name) = updates.get("name").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id])?;
    }
    // 更新 apiKey（加密后存储）。
    //
    // 三种语义必须区分清楚，否则「编辑时没动密钥输入框」会被误判成「清空密钥」：
    //   - 字段不存在（None）      → 不修改，保留原值
    //   - 空字符串 ""            → 不修改，保留原值（前端「留空保持不变」的约定）
    //   - 显式 null              → 明确清空
    // 旧实现把 "" 也走 encrypt_value（空串返回 None），等于把已有密钥写成 NULL。
    match updates.get("apiKey") {
        Some(v) if v.is_null() => {
            conn.execute(
                "UPDATE provider_connections SET api_key = NULL, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
        }
        Some(v) => {
            if let Some(api_key) = v.as_str() {
                let trimmed = api_key.trim();
                if !trimmed.is_empty() {
                    let encrypted = encrypt_value(enc_key, trimmed);
                    conn.execute(
                        "UPDATE provider_connections SET api_key = ?1, updated_at = ?2 WHERE id = ?3",
                        params![encrypted, now, id],
                    )?;
                }
            }
        }
        None => {}
    }
    // 更新 isActive
    if let Some(is_active) = updates.get("isActive").and_then(|v| v.as_bool()) {
        conn.execute("UPDATE provider_connections SET is_active = ?1, updated_at = ?2 WHERE id = ?3", params![is_active as i32, now, id])?;
    }
    // 更新 priority
    if let Some(priority) = updates.get("priority").and_then(|v| v.as_i64()) {
        conn.execute("UPDATE provider_connections SET priority = ?1, updated_at = ?2 WHERE id = ?3", params![priority as i32, now, id])?;
    }
    // 更新 defaultModel
    if let Some(default_model) = updates.get("defaultModel").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET default_model = ?1, updated_at = ?2 WHERE id = ?3", params![default_model, now, id])?;
    }
    // defaultModel 为 null 时清除
    if updates.get("defaultModel").map(|v| v.is_null()).unwrap_or(false) {
        conn.execute("UPDATE provider_connections SET default_model = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    // 更新 email
    if let Some(email) = updates.get("email").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET email = ?1, updated_at = ?2 WHERE id = ?3", params![email, now, id])?;
    }
    if updates.get("email").map(|v| v.is_null()).unwrap_or(false) {
        conn.execute("UPDATE provider_connections SET email = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    // 更新 projectId
    if let Some(project_id) = updates.get("projectId").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET project_id = ?1, updated_at = ?2 WHERE id = ?3", params![project_id, now, id])?;
    }
    if updates.get("projectId").map(|v| v.is_null()).unwrap_or(false) {
        conn.execute("UPDATE provider_connections SET project_id = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    // 更新 groupName
    if let Some(group_name) = updates.get("groupName").and_then(|v| v.as_str()) {
        conn.execute("UPDATE provider_connections SET group_name = ?1, updated_at = ?2 WHERE id = ?3", params![group_name, now, id])?;
    }
    if updates.get("groupName").map(|v| v.is_null()).unwrap_or(false) {
        conn.execute("UPDATE provider_connections SET group_name = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    // 更新 maxConcurrent
    if let Some(mc) = updates.get("maxConcurrent").and_then(|v| v.as_i64()) {
        conn.execute("UPDATE provider_connections SET max_concurrent = ?1, updated_at = ?2 WHERE id = ?3", params![mc as i32, now, id])?;
    }
    if updates.get("maxConcurrent").map(|v| v.is_null()).unwrap_or(false) {
        conn.execute("UPDATE provider_connections SET max_concurrent = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
    }
    // 更新 rateLimitProtection
    if let Some(rp) = updates.get("rateLimitProtection").and_then(|v| v.as_bool()) {
        conn.execute("UPDATE provider_connections SET rate_limit_protection = ?1, updated_at = ?2 WHERE id = ?3", params![rp as i32, now, id])?;
    }
    // 更新 proxyEnabled
    if let Some(pe) = updates.get("proxyEnabled").and_then(|v| v.as_bool()) {
        conn.execute("UPDATE provider_connections SET proxy_enabled = ?1, updated_at = ?2 WHERE id = ?3", params![pe as i32, now, id])?;
    }
    // 更新 healthCheckInterval
    if let Some(hci) = updates.get("healthCheckInterval").and_then(|v| v.as_i64()) {
        conn.execute("UPDATE provider_connections SET health_check_interval = ?1, updated_at = ?2 WHERE id = ?3", params![hci as i32, now, id])?;
    }
    // 更新 baseUrl（写入 provider_specific_data JSON）
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
    // 更新 chatPath（写入 provider_specific_data JSON）：自定义提供方可用它覆盖默认
    // /v1/chat/completions 路径（如 z.ai Coding Plan 实际路径为 /chat/completions，无 /v1 前缀）。
    if let Some(chat_path) = updates.get("chatPath").and_then(|v| v.as_str()) {
        let existing = get_by_id(conn, id, enc_key)?;
        if let Some(mut p) = existing {
            let mut data = p.provider_specific_data.as_object_mut().cloned().unwrap_or_default();
            data.insert("chatPath".to_string(), serde_json::Value::String(chat_path.to_string()));
            let data_str = serde_json::to_string(&data)?;
            conn.execute("UPDATE provider_connections SET provider_specific_data = ?1, updated_at = ?2 WHERE id = ?3", params![data_str, now, id])?;
        }
    }
    // displayName / apiProtocol：列与 JSON 双写。列给列表展示，JSON 给代理层读取。
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
            let data_str = serde_json::to_string(&data)?;
            conn.execute("UPDATE provider_connections SET provider_specific_data = ?1, updated_at = ?2 WHERE id = ?3", params![data_str, now, id])?;
        }
    }
    // models：存入 provider_specific_data["models"]，并（未显式指定默认模型时）
    // 同步 default_model 为首模型，维持代理层路由回退行为。
    if let Some(models_val) = updates.get("models") {
        if models_val.is_null() {
            // models 为 null：从 JSON 中移除 models 并清除默认模型
            let existing = get_by_id(conn, id, enc_key)?;
            if let Some(mut p) = existing {
                let mut data = p.provider_specific_data.as_object_mut().cloned().unwrap_or_default();
                data.remove("models");
                let data_str = serde_json::to_string(&data)?;
                conn.execute("UPDATE provider_connections SET provider_specific_data = ?1, updated_at = ?2 WHERE id = ?3", params![data_str, now, id])?;
            }
            if updates.get("defaultModel").is_none() {
                conn.execute("UPDATE provider_connections SET default_model = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
            }
        } else {
            // 解析模型列表并写入 JSON
            let parsed: Vec<ProviderModel> = serde_json::from_value(models_val.clone()).unwrap_or_default();
            let existing = get_by_id(conn, id, enc_key)?;
            if let Some(mut p) = existing {
                let mut data = p.provider_specific_data.as_object_mut().cloned().unwrap_or_default();
                data.insert(
                    "models".to_string(),
                    serde_json::to_value(&parsed).unwrap_or(serde_json::Value::Null),
                );
                let data_str = serde_json::to_string(&data)?;
                conn.execute("UPDATE provider_connections SET provider_specific_data = ?1, updated_at = ?2 WHERE id = ?3", params![data_str, now, id])?;
            }
            // 未显式指定默认模型时，同步为模型列表首个
            if updates.get("defaultModel").is_none() {
                match parsed.first() {
                    Some(first) => {
                        conn.execute("UPDATE provider_connections SET default_model = ?1, updated_at = ?2 WHERE id = ?3", params![first.id, now, id])?;
                    }
                    None => {
                        conn.execute("UPDATE provider_connections SET default_model = NULL, updated_at = ?1 WHERE id = ?2", params![now, id])?;
                    }
                }
            }
        }
    }

    // 返回更新后的连接
    get_by_id(conn, id, enc_key)
}

/// 删除提供商连接。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
///
/// # 返回
/// 删除成功返回 `true`，连接不存在返回 `false`
pub fn delete(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM provider_connections WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

/// 更新连接的测试状态。
///
/// 记录测试结果（`ok` / `fail`）、错误信息及测试时间。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
/// - `status`：测试状态字符串
/// - `error`：错误信息（可选）
/// - `latency_ms`：响应延迟（毫秒，可选；失败时传 None 会一并清空旧值）
pub fn update_test_status(
    conn: &rusqlite::Connection,
    id: &str,
    status: &str,
    error: Option<&str>,
    latency_ms: Option<i64>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET test_status = ?1, last_error = ?2, last_tested = ?3, updated_at = ?4, last_latency_ms = ?5 WHERE id = ?6",
        params![status, error, now, now, latency_ms, id],
    )?;
    Ok(())
}

/// 递增连接的使用计数并更新最近使用时间。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
pub fn increment_usage(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET consecutive_use_count = consecutive_use_count + 1, last_used_at = ?1, updated_at = ?2 WHERE id = ?3",
        params![now, now, id],
    )?;
    Ok(())
}

/// 重置连接的退避状态。
///
/// 将退避级别归零、清除限流恢复时间、重置连续使用计数。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
pub fn reset_backoff(conn: &rusqlite::Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET backoff_level = 0, rate_limited_until = NULL, consecutive_use_count = 0, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

/// 递增连接的退避级别并设置限流恢复时间。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：连接唯一标识
/// - `rate_limited_until`：限流恢复时间（可选，None 表示不设置）
pub fn increment_backoff(conn: &rusqlite::Connection, id: &str, rate_limited_until: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_connections SET backoff_level = backoff_level + 1, rate_limited_until = ?1, updated_at = ?2 WHERE id = ?3",
        params![rate_limited_until, now, id],
    )?;
    Ok(())
}

/// 将数据库行映射为 `ProviderConnection` 实体。
///
/// 解析 `provider_specific_data` JSON 提取模型列表，解密敏感字段
/// （API Key、Access Token、Refresh Token）。
///
/// # 参数
/// - `row`：数据库行引用
/// - `enc_key`：解密密钥
///
/// # 返回
/// 映射后的连接实体（rusqlite 错误类型）
fn row_to_connection(row: &Row, enc_key: &[u8]) -> rusqlite::Result<ProviderConnection> {
    // 解析 provider_specific_data JSON
    let specific_data_str: String = row.get(32)?;
    let specific_data = serde_json::from_str(&specific_data_str).unwrap_or(serde_json::json!({}));

    // 从 JSON 中提取模型列表
    let models: Option<Vec<ProviderModel>> = specific_data
        .get("models")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    // 解密敏感字段
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
        models,
        token_type: row.get(27)?,
        scope: row.get(28)?,
        last_used_at: row.get(29)?,
        last_health_check_at: row.get(30)?,
        last_tested: row.get(31)?,
        provider_specific_data: specific_data,
        created_at: row.get(33)?,
        updated_at: row.get(34)?,
        // 迁移 004 新增列；老库未应用迁移时用 unwrap_or 兜底，避免整表查询失败
        last_latency_ms: row.get::<_, Option<i64>>(35).unwrap_or(None),
    })
}
