//! API 密钥 CRUD 操作模块。
//!
//! 负责对 `api_keys` 表的增删改查。密钥值格式为 `vx-{uuid}`（去掉连字符）。
//! 创建时默认启用自动路由解析（`auto_resolve = true`）、启用且未封禁。

use rusqlite::{params, Row};
use crate::db::models::{ApiKey, CreateApiKeyRequest};
use crate::error::Result;

/// 查询列名常量，供各查询复用
const SELECT_COLS: &str = "id, name, key, machine_id, allowed_models, \
     allowed_connections, allowed_endpoints, no_log, auto_resolve, \
     is_active, is_banned, rate_limits, usage_limits, created_at";

/// 查询所有 API 密钥列表。
///
/// 按创建时间降序排列。
///
/// # 参数
/// - `conn`：数据库连接
///
/// # 返回
/// 所有密钥的列表
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

/// 按 ID 查询单个 API 密钥。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：密钥唯一标识
///
/// # 返回
/// 找到返回 `Some(密钥)`，未找到返回 `None`
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

/// 按密钥值查询活跃且未封禁的 API 密钥。
///
/// 仅返回 `is_active = 1 AND is_banned = 0` 的密钥，用于请求鉴权时校验。
///
/// # 参数
/// - `conn`：数据库连接
/// - `key`：密钥值（`vx-...` 格式）
///
/// # 返回
/// 找到返回 `Some(密钥)`，未找到返回 `None`
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

/// 创建新的 API 密钥。
///
/// 生成 UUID 作为主键，密钥值格式为 `vx-{uuid}`（去掉连字符）。
/// 默认 `auto_resolve = true`、`is_active = true`、`is_banned = false`。
///
/// # 参数
/// - `conn`：数据库连接
/// - `req`：创建请求参数
///
/// # 返回
/// 创建成功的密钥实体
pub fn create(conn: &rusqlite::Connection, req: &CreateApiKeyRequest) -> Result<ApiKey> {
    let id = uuid::Uuid::new_v4().to_string();
    // 生成密钥值：vx- 前缀 + 去掉连字符的 UUID
    let key = format!("vx-{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let now = chrono::Utc::now().to_rfc3339();

    // 序列化 JSON 字段，提供默认值
    let allowed_models = serde_json::to_string(&req.allowed_models.clone().unwrap_or(serde_json::json!([])))?;
    let rate_limits = serde_json::to_string(&req.rate_limits.clone().unwrap_or(serde_json::json!([])))?;
    let usage_limits = serde_json::to_string(&req.usage_limits.clone().unwrap_or(serde_json::json!({})))?;

    // 插入数据库
    conn.execute(
        "INSERT INTO api_keys (id, name, key, allowed_models, \
         allowed_connections, allowed_endpoints, no_log, auto_resolve, \
         is_active, is_banned, rate_limits, usage_limits, created_at) \
         VALUES (?1, ?2, ?3, ?4, '[]', '[]', 0, 1, 1, 0, ?5, ?6, ?7)",
        params![id, req.name, key, allowed_models, rate_limits, usage_limits, now],
    )?;

    // 返回创建的密钥实体
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

/// 删除 API 密钥。
///
/// # 参数
/// - `conn`：数据库连接
/// - `id`：密钥唯一标识
///
/// # 返回
/// 删除成功返回 `true`，密钥不存在返回 `false`
pub fn delete(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM api_keys WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

/// 将数据库行映射为 `ApiKey` 实体。
///
/// 解析 JSON 字段（allowed_models、allowed_connections、allowed_endpoints、
/// rate_limits、usage_limits），将布尔列从整数还原。
///
/// # 参数
/// - `row`：数据库行引用
///
/// # 返回
/// 映射后的密钥实体（rusqlite 错误类型）
fn row_to_key(row: &Row) -> rusqlite::Result<ApiKey> {
    // 读取 JSON 字符串字段
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
        // 解析 JSON，失败时回退到默认值
        allowed_models: serde_json::from_str(&allowed_models_str).unwrap_or(serde_json::json!([])),
        allowed_connections: serde_json::from_str(&allowed_conn_str).unwrap_or(serde_json::json!([])),
        allowed_endpoints: serde_json::from_str(&allowed_endpoints_str).unwrap_or(serde_json::json!([])),
        // 布尔列从整数还原
        no_log: row.get::<_, i32>(7)? != 0,
        auto_resolve: row.get::<_, i32>(8)? != 0,
        is_active: row.get::<_, i32>(9)? != 0,
        is_banned: row.get::<_, i32>(10)? != 0,
        rate_limits: serde_json::from_str(&rate_limits_str).unwrap_or(serde_json::json!([])),
        usage_limits: serde_json::from_str(&usage_limits_str).unwrap_or(serde_json::json!({})),
        created_at: row.get(13)?,
    })
}
