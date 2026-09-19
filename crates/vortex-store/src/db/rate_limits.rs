//! 速率限制配置与用量 CRUD。
//!
//! 管理两类数据：
//! - `rate_limit_caps`：每个 provider+model+connection 的 RPM/RPD/TPM/TPD 限额配置
//! - `rate_limit_usage`：每次请求的 token 用量明细，用于滑动窗口计算

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 速率限制配置，对应 `rate_limit_caps` 表
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitCap {
    pub id: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpd: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpd: Option<i64>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建/更新速率限制配置的请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertRateLimitCap {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub rpm: Option<i64>,
    #[serde(default)]
    pub rpd: Option<i64>,
    #[serde(default)]
    pub tpm: Option<i64>,
    #[serde(default)]
    pub tpd: Option<i64>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

/// 速率用量快照：某 provider+model+connection 在各时间窗口内的用量
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateUsageSnapshot {
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// 当前分钟内请求数
    pub current_rpm: i64,
    /// 当前天内请求数
    pub current_rpd: i64,
    /// 当前分钟内 token 数
    pub current_tpm: i64,
    /// 当前天内 token 数
    pub current_tpd: i64,
    /// 配置的限额（如有）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_rpm: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_rpd: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_tpm: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_tpd: Option<i64>,
}

/// 列出所有速率限制配置
pub fn list_caps(conn: &Connection) -> Result<Vec<RateLimitCap>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model, connection_id, rpm, rpd, tpm, tpd, is_active, source, created_at, updated_at
         FROM rate_limit_caps ORDER BY provider, model",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(RateLimitCap {
            id: row.get(0)?,
            provider: row.get(1)?,
            model: row.get(2)?,
            connection_id: row.get(3)?,
            rpm: row.get(4)?,
            rpd: row.get(5)?,
            tpm: row.get(6)?,
            tpd: row.get(7)?,
            is_active: row.get::<_, i32>(8)? != 0,
            source: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 按 provider+model+connection 查找配置
pub fn get_cap(conn: &Connection, provider: &str, model: &str, connection_id: Option<&str>) -> Result<Option<RateLimitCap>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model, connection_id, rpm, rpd, tpm, tpd, is_active, source, created_at, updated_at
         FROM rate_limit_caps WHERE provider = ?1 AND model = ?2 AND connection_id IS ?3",
    )?;
    let mut rows = stmt.query_map(params![provider, model, connection_id], |row| {
        Ok(RateLimitCap {
            id: row.get(0)?,
            provider: row.get(1)?,
            model: row.get(2)?,
            connection_id: row.get(3)?,
            rpm: row.get(4)?,
            rpd: row.get(5)?,
            tpm: row.get(6)?,
            tpd: row.get(7)?,
            is_active: row.get::<_, i32>(8)? != 0,
            source: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}

/// 插入或更新速率限制配置（UPSERT）
pub fn upsert_cap(conn: &Connection, req: &UpsertRateLimitCap) -> Result<RateLimitCap> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let is_active = req.is_active.unwrap_or(true);
    conn.execute(
        "INSERT INTO rate_limit_caps (id, provider, model, connection_id, rpm, rpd, tpm, tpd, is_active, source, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'manual', ?10, ?10)
         ON CONFLICT(provider, model, connection_id) DO UPDATE SET
            rpm = excluded.rpm, rpd = excluded.rpd, tpm = excluded.tpm, tpd = excluded.tpd,
            is_active = excluded.is_active, updated_at = excluded.updated_at",
        params![id, req.provider, req.model, req.connection_id, req.rpm, req.rpd, req.tpm, req.tpd, is_active as i32, now],
    )?;
    get_cap(conn, &req.provider, &req.model, req.connection_id.as_deref())?
        .ok_or_else(|| crate::error::AppError::Internal("Failed to upsert rate limit cap".into()))
}

/// 删除速率限制配置
pub fn delete_cap(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM rate_limit_caps WHERE id = ?1", params![id])?;
    Ok(())
}

/// 记算某 provider+model+connection 在各时间窗口内的用量
pub fn get_usage_snapshot(conn: &Connection, provider: &str, model: &str, connection_id: Option<&str>) -> Result<RateUsageSnapshot> {
    let now = chrono::Utc::now();
    let one_min_ago = (now - chrono::Duration::minutes(1)).to_rfc3339();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc().to_rfc3339();

    // 当前分钟内请求数
    let current_rpm: i64 = conn.query_row(
        "SELECT COUNT(*) FROM rate_limit_usage WHERE provider = ?1 AND model = ?2 AND connection_id IS ?3 AND timestamp >= ?4",
        params![provider, model, connection_id, one_min_ago],
        |row| row.get(0),
    )?;

    // 当前天内请求数
    let current_rpd: i64 = conn.query_row(
        "SELECT COUNT(*) FROM rate_limit_usage WHERE provider = ?1 AND model = ?2 AND connection_id IS ?3 AND timestamp >= ?4",
        params![provider, model, connection_id, today_start],
        |row| row.get(0),
    )?;

    // 当前分钟内 token 数
    let current_tpm: i64 = conn.query_row(
        "SELECT COALESCE(SUM(tokens), 0) FROM rate_limit_usage WHERE provider = ?1 AND model = ?2 AND connection_id IS ?3 AND timestamp >= ?4",
        params![provider, model, connection_id, one_min_ago],
        |row| row.get(0),
    ).unwrap_or(0);

    // 当前天内 token 数
    let current_tpd: i64 = conn.query_row(
        "SELECT COALESCE(SUM(tokens), 0) FROM rate_limit_usage WHERE provider = ?1 AND model = ?2 AND connection_id IS ?3 AND timestamp >= ?4",
        params![provider, model, connection_id, today_start],
        |row| row.get(0),
    ).unwrap_or(0);

    // 查配置
    let cap = get_cap(conn, provider, model, connection_id)?;

    Ok(RateUsageSnapshot {
        provider: provider.to_string(),
        model: model.to_string(),
        connection_id: connection_id.map(|s| s.to_string()),
        current_rpm,
        current_rpd,
        current_tpm,
        current_tpd,
        cap_rpm: cap.as_ref().and_then(|c| c.rpm),
        cap_rpd: cap.as_ref().and_then(|c| c.rpd),
        cap_tpm: cap.as_ref().and_then(|c| c.tpm),
        cap_tpd: cap.as_ref().and_then(|c| c.tpd),
    })
}

/// 记算某 provider+model+connection 在各时间窗口内的用量（带配置上限）
#[allow(dead_code)]
pub fn check_rate_limit(conn: &Connection, provider: &str, model: &str, connection_id: Option<&str>, estimated_tokens: i64) -> Result<bool> {
    let cap = match get_cap(conn, provider, model, connection_id)? {
        Some(c) if c.is_active => c,
        _ => return Ok(true),
    };
    let snapshot = get_usage_snapshot(conn, provider, model, connection_id)?;
    if let Some(rpm) = cap.rpm
        && snapshot.current_rpm >= rpm {
            return Ok(false);
        }
    if let Some(rpd) = cap.rpd
        && snapshot.current_rpd >= rpd {
            return Ok(false);
        }
    if let Some(tpm) = cap.tpm
        && snapshot.current_tpm + estimated_tokens > tpm {
            return Ok(false);
        }
    if let Some(tpd) = cap.tpd
        && snapshot.current_tpd + estimated_tokens > tpd {
            return Ok(false);
        }
    Ok(true)
}

/// 记算某 provider+model+connection 在各时间窗口内的用量（带配置上限）并返回剩余配额
#[allow(dead_code)]
pub fn get_remaining_quota(conn: &Connection, provider: &str, model: &str, connection_id: Option<&str>) -> Result<Option<RateUsageSnapshot>> {
    let cap = get_cap(conn, provider, model, connection_id)?;
    if cap.is_none() || !cap.as_ref().map(|c| c.is_active).unwrap_or(false) {
        return Ok(None);
    }
    let snapshot = get_usage_snapshot(conn, provider, model, connection_id)?;
    Ok(Some(snapshot))
}

/// 记算某 provider+model+connection 在各时间窗口内的用量
pub fn record_usage(conn: &Connection, provider: &str, model: &str, connection_id: Option<&str>, tokens: i64) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO rate_limit_usage (provider, model, connection_id, tokens, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![provider, model, connection_id, tokens, now],
    )?;
    Ok(())
}

/// 清理过期用量记录（保留最近 48 小时）
pub fn cleanup_old_usage(conn: &Connection) -> Result<usize> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    let affected = conn.execute(
        "DELETE FROM rate_limit_usage WHERE timestamp < ?1",
        params![cutoff],
    )?;
    Ok(affected)
}

/// 批量获取所有活跃配置的用量快照
pub fn list_usage_snapshots(conn: &Connection) -> Result<Vec<RateUsageSnapshot>> {
    let caps = list_caps(conn)?;
    let mut snapshots = Vec::new();
    for cap in &caps {
        if !cap.is_active {
            continue;
        }
        let snapshot = get_usage_snapshot(conn, &cap.provider, &cap.model, cap.connection_id.as_deref())?;
        snapshots.push(snapshot);
    }
    Ok(snapshots)
}
