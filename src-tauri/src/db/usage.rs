use rusqlite::params;
use crate::db::models::{UsageEntry, UsageStats};
use crate::error::Result;

pub fn record(conn: &rusqlite::Connection, entry: &UsageEntry) -> Result<()> {
    conn.execute(
        "INSERT INTO usage_history \
         (provider, model, connection_id, api_key_id, api_key_name, \
         tokens_input, tokens_output, tokens_cache_read, tokens_cache_creation, \
         tokens_reasoning, service_tier, status, success, error_code, \
         latency_ms, ttft_ms, cost, timestamp) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            entry.provider, entry.model, entry.connection_id,
            entry.api_key_id, entry.api_key_name,
            entry.tokens_input, entry.tokens_output, entry.tokens_cache_read,
            entry.tokens_cache_creation, entry.tokens_reasoning,
            entry.service_tier, entry.status, entry.success as i32,
            entry.error_code, entry.latency_ms, entry.ttft_ms,
            entry.cost, entry.timestamp
        ],
    )?;
    Ok(())
}

pub fn get_stats(conn: &rusqlite::Connection, since: Option<&str>) -> Result<UsageStats> {
    let where_clause = since.map(|s| format!("WHERE timestamp >= '{}'", s.replace('\'', "''"))).unwrap_or_default();

    let total_requests: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM usage_history {}", where_clause), [], |row| row.get(0)
    ).unwrap_or(0);

    let total_tokens_input: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(tokens_input), 0) FROM usage_history {}", where_clause), [], |row| row.get(0)
    ).unwrap_or(0);

    let total_tokens_output: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(tokens_output), 0) FROM usage_history {}", where_clause), [], |row| row.get(0)
    ).unwrap_or(0);

    let total_cost: f64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(cost), 0.0) FROM usage_history {}", where_clause), [], |row| row.get(0)
    ).unwrap_or(0.0);

    let avg_latency: f64 = conn.query_row(
        &format!("SELECT COALESCE(AVG(latency_ms), 0.0) FROM usage_history {} WHERE latency_ms IS NOT NULL", where_clause), [], |row| row.get(0)
    ).unwrap_or(0.0);

    let success_count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM usage_history {} WHERE success = 1", where_clause), [], |row| row.get(0)
    ).unwrap_or(0);

    let success_rate = if total_requests > 0 {
        success_count as f64 / total_requests as f64
    } else {
        1.0
    };

    let mut by_provider = serde_json::Map::new();
    let sql = format!("SELECT provider, COUNT(*) as cnt FROM usage_history {} GROUP BY provider", where_clause);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let provider: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((provider, count))
    })?;
    for row in rows {
        let (provider, count) = row?;
        by_provider.insert(provider, serde_json::Value::Number(count.into()));
    }

    let mut by_model = serde_json::Map::new();
    let sql2 = format!("SELECT model, COUNT(*) as cnt FROM usage_history {} GROUP BY model", where_clause);
    let mut stmt2 = conn.prepare(&sql2)?;
    let rows2 = stmt2.query_map([], |row| {
        let model: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((model, count))
    })?;
    for row in rows2 {
        let (model, count) = row?;
        by_model.insert(model, serde_json::Value::Number(count.into()));
    }

    let mut by_day = serde_json::Map::new();
    let sql3 = format!("SELECT DATE(timestamp) as day, COUNT(*) as cnt, COALESCE(SUM(tokens_input), 0) as tin, COALESCE(SUM(tokens_output), 0) as tout FROM usage_history {} GROUP BY DATE(timestamp) ORDER BY day DESC LIMIT 30", where_clause);
    let mut stmt3 = conn.prepare(&sql3)?;
    let rows3 = stmt3.query_map([], |row| {
        let day: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        let tin: i64 = row.get(2)?;
        let tout: i64 = row.get(3)?;
        Ok((day, count, tin, tout))
    })?;
    for row in rows3 {
        let (day, count, tin, tout) = row?;
        by_day.insert(day, serde_json::json!({
            "requests": count,
            "tokens_input": tin,
            "tokens_output": tout
        }));
    }

    Ok(UsageStats {
        total_requests,
        total_tokens_input,
        total_tokens_output,
        total_cost,
        avg_latency_ms: avg_latency,
        success_rate,
        by_provider: serde_json::Value::Object(by_provider),
        by_model: serde_json::Value::Object(by_model),
        by_day: serde_json::Value::Object(by_day),
    })
}

pub fn list_recent(conn: &rusqlite::Connection, limit: i64) -> Result<Vec<UsageEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model, connection_id, api_key_id, api_key_name, \
         tokens_input, tokens_output, tokens_cache_read, tokens_cache_creation, \
         tokens_reasoning, service_tier, status, success, error_code, \
         latency_ms, ttft_ms, cost, timestamp \
         FROM usage_history ORDER BY timestamp DESC LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(UsageEntry {
            id: row.get(0)?,
            provider: row.get(1)?,
            model: row.get(2)?,
            connection_id: row.get(3)?,
            api_key_id: row.get(4)?,
            api_key_name: row.get(5)?,
            tokens_input: row.get(6)?,
            tokens_output: row.get(7)?,
            tokens_cache_read: row.get(8)?,
            tokens_cache_creation: row.get(9)?,
            tokens_reasoning: row.get(10)?,
            service_tier: row.get(11)?,
            status: row.get(12)?,
            success: row.get::<_, i32>(13)? != 0,
            error_code: row.get(14)?,
            latency_ms: row.get(15)?,
            ttft_ms: row.get(16)?,
            cost: row.get(17)?,
            timestamp: row.get(18)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
