//! 延迟统计端点（p50/p95/TTFT）。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

pub async fn latency_stats(
    state: web::Data<std::sync::Arc<AppState>>,
    query: web::Query<LatencyQuery>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let since = query.since.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;

        let since_clause = match since.as_deref() {
            Some(s) => format!("AND timestamp >= '{}'", s.replace('\'', "''")),
            None => String::new(),
        };

        let mut latencies: Vec<f64> = Vec::new();
        let lat_query = format!(
            "SELECT latency_ms FROM usage_history WHERE success = 1 AND latency_ms IS NOT NULL {} ORDER BY latency_ms",
            since_clause
        );
        let mut stmt = conn.prepare(&lat_query).map_err(|e| e.to_string())?;
        let lat_iter = stmt.query_map([], |row| row.get::<_, f64>(0)).map_err(|e| e.to_string())?;
        for v in lat_iter.flatten() {
            latencies.push(v);
        }

        let mut ttfts: Vec<f64> = Vec::new();
        let ttft_query = format!(
            "SELECT ttft_ms FROM usage_history WHERE success = 1 AND ttft_ms IS NOT NULL {} ORDER BY ttft_ms",
            since_clause
        );
        let mut stmt2 = conn.prepare(&ttft_query).map_err(|e| e.to_string())?;
        let ttft_iter = stmt2.query_map([], |row| row.get::<_, f64>(0)).map_err(|e| e.to_string())?;
        for v in ttft_iter.flatten() {
            ttfts.push(v);
        }

        let lat_stats = compute_stats(&latencies);
        let ttft_stats = compute_stats(&ttfts);

        let by_provider_query = format!(
            "SELECT
                COALESCE(provider, 'unknown') as provider,
                AVG(latency_ms) as avg,
                COUNT(*) as count
            FROM usage_history
            WHERE success = 1 AND latency_ms IS NOT NULL {}
            GROUP BY provider
            ORDER BY count DESC",
            since_clause
        );
        let mut stmt3 = conn.prepare(&by_provider_query).map_err(|e| e.to_string())?;
        let providers: Vec<serde_json::Value> = stmt3.query_map([], |row| {
            let provider: String = row.get(0)?;
            let avg: f64 = row.get(1)?;
            let count: i64 = row.get(2)?;
            Ok(json!({
                "provider": provider,
                "avg": avg,
                "count": count,
            }))
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        Ok(json!({
            "latency": lat_stats,
            "ttft": ttft_stats,
            "byProvider": providers,
        }))
    })
    .await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}

fn compute_stats(sorted: &[f64]) -> serde_json::Value {
    let n = sorted.len();
    if n == 0 {
        return json!({ "p50": 0, "p95": 0, "p99": 0, "avg": 0, "min": 0, "max": 0, "count": 0 });
    }
    let p50 = sorted[(n as f64 * 0.50) as usize % n];
    let p95 = sorted[(n as f64 * 0.95) as usize % n];
    let p99 = sorted[(n as f64 * 0.99) as usize % n];
    let avg = sorted.iter().sum::<f64>() / n as f64;
    let min = sorted[0];
    let max = sorted[n - 1];
    json!({ "p50": p50, "p95": p95, "p99": p99, "avg": avg, "min": min, "max": max, "count": n })
}

#[derive(Debug, serde::Deserialize)]
pub struct LatencyQuery {
    #[serde(default)]
    since: Option<String>,
}
