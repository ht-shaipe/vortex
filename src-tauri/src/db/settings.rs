//! 设置存储模块。
//!
//! 基于 `key_value` 表实现通用键值存储，以 `namespace + key` 作为联合主键。
//! 当前主要服务于 `settings` 命名空间下的 `general`（界面/端口）和 `security`（鉴权与跨域）子段。
//! 支持部分更新（浅合并），避免前端只传单个字段时误清空其它已存配置。
//! 另提供管理 API 鉴权口令的生成功能。

use rusqlite::{params};
use crate::error::Result;

/// 读取指定命名空间和键的值。
///
/// # 参数
/// - `conn`：数据库连接
/// - `namespace`：命名空间
/// - `key`：键名
///
/// # 返回
/// 找到返回 `Some(JSON 值)`，未找到返回 `None`；JSON 解析失败时回退为空对象
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

/// 写入指定命名空间和键的值（存在则更新）。
///
/// # 参数
/// - `conn`：数据库连接
/// - `namespace`：命名空间
/// - `key`：键名
/// - `value`：JSON 值
///
/// # 返回
/// 成功返回 `Ok(())`
pub fn set(conn: &rusqlite::Connection, namespace: &str, key: &str, value: &serde_json::Value) -> Result<()> {
    let value_str = serde_json::to_string(value)?;
    // UPSERT：存在则更新，不存在则插入
    conn.execute(
        "INSERT INTO key_value (namespace, key, value) VALUES (?1, ?2, ?3) \
         ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value",
        params![namespace, key, value_str],
    )?;
    Ok(())
}

/// 汇总 settings 命名空间下的所有子段返回给前端。
/// 当前已注册：`general`（界面/端口）、`security`（鉴权与跨域）。
///
/// # 参数
/// - `conn`：数据库连接
///
/// # 返回
/// 包含 `general` 和 `security` 子对象的 JSON 值
pub fn get_settings(conn: &rusqlite::Connection) -> Result<serde_json::Value> {
    let general = get(conn, "settings", "general")?.unwrap_or(serde_json::json!({}));
    let security = get(conn, "settings", "security")?.unwrap_or(serde_json::json!({}));
    Ok(serde_json::json!({
        "general": general,
        "security": security,
    }))
}

/// 接受部分更新。前端传 `{ general?: {...}, security?: {...} }`，每个顶层 key
/// 都对应 settings 命名空间下的一个子段。
/// 与 `general` / `security` 子对象做浅合并后再写入，避免前端只传单个字段时
/// 误清空其它已存配置。空对象会跳过。
///
/// # 参数
/// - `conn`：数据库连接
/// - `updates`：更新内容 JSON（顶层 key 对应子段名）
///
/// # 返回
/// 更新后的完整设置（等同 `get_settings` 的结果）
pub fn update_settings(conn: &rusqlite::Connection, updates: &serde_json::Value) -> Result<serde_json::Value> {
    if let Some(obj) = updates.as_object() {
        for (key, value) in obj {
            // 跳过非对象值
            if !value.is_object() {
                continue;
            }
            // 读取当前值并与新值浅合并
            let current = get(conn, "settings", key)?.unwrap_or(serde_json::json!({}));
            let merged = merge_objects(&current, value);
            set(conn, "settings", key, &merged)?;
        }
    }
    // 返回更新后的完整设置
    get_settings(conn)
}

/// 浅合并两个 JSON 对象。
///
/// 将 `b` 的所有键值对覆盖到 `a` 的副本中。若 `a` 不是对象则直接返回 `b`。
///
/// # 参数
/// - `a`：基础 JSON 值
/// - `b`：覆盖 JSON 值
///
/// # 返回
/// 合并后的 JSON 值
fn merge_objects(a: &serde_json::Value, b: &serde_json::Value) -> serde_json::Value {
    let mut out = a.clone();
    if let (Some(out_obj), Some(b_obj)) = (out.as_object_mut(), b.as_object()) {
        for (k, v) in b_obj {
            out_obj.insert(k.clone(), v.clone());
        }
    } else {
        out = b.clone();
    }
    out
}

/// 生成 32 字符（16 字节）十六进制 token，格式与 `d5b6663a339244e4be08a7d28e766e05` 一致。
/// 不引入额外依赖：混合时间戳纳秒 / 进程 ID / DefaultHasher 输出作为熵源。
/// 用于管理 API 的鉴权口令，强度足够非公开网络环境的局域网代理场景。
///
/// # 返回
/// 32 字符小写十六进制字符串
#[allow(dead_code)]
pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    // 获取当前时间纳秒作为熵源
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u64;

    // 第一轮哈希：纳秒 + PID
    let mut h1 = DefaultHasher::new();
    h1.write_u128(nanos);
    h1.write_u64(pid);
    let a = h1.finish();

    // 第二轮哈希：对熵源做变换后再哈希，增加散列差异
    let mut h2 = DefaultHasher::new();
    h2.write_u128(nanos.rotate_left(17).wrapping_add(0x9E3779B97F4A7C15));
    h2.write_u64(pid.rotate_left(11) ^ (nanos as u64));
    let b = h2.finish();

    // 拼接两轮哈希结果为 16 字节
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&a.to_be_bytes());
    bytes[8..].copy_from_slice(&b.to_be_bytes());
    hex_lower(&bytes)
}

/// 将字节数组转为小写十六进制字符串。
///
/// # 参数
/// - `bytes`：任意长度字节数组
///
/// # 返回
/// 小写十六进制字符串（长度为输入字节数的 2 倍）
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证生成的 token 为 32 位十六进制且两次生成结果不同
    #[test]
    fn generate_token_is_hex_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32, "token should be 32 hex chars, got {} (={})", a.len(), a);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "non-hex chars in token: {}", a);
        assert_ne!(a, b, "two consecutive tokens must differ");
    }
}
