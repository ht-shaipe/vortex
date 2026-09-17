//! 智能推荐端点：分析使用历史生成优化建议。
use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

pub async fn recommendations(
    state: web::Data<std::sync::Arc<AppState>>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = crate::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let mut suggestions = Vec::new();

        // 1. 频繁失败的模型
        let mut stmt = conn.prepare(
            "SELECT model, provider, COUNT(*) as cnt \
             FROM usage_history WHERE success = 0 AND model IS NOT NULL \
             GROUP BY model HAVING cnt >= 3 ORDER BY cnt DESC LIMIT 5"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(json!({
                "type": "frequent_failures",
                "severity": "high",
                "model": row.get::<_, String>(0)?,
                "provider": row.get::<_, Option<String>>(1)?,
                "count": row.get::<_, i64>(2)?,
                "message": format!("模型 {} 最近频繁失败，建议检查连接或切换替代模型", row.get::<_, String>(0)?),
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows { suggestions.push(row.map_err(|e| e.to_string())?); }

        // 2. 高成本模型
        let mut stmt2 = conn.prepare(
            "SELECT model, SUM(cost) as total_cost, COUNT(*) as cnt \
             FROM usage_history WHERE model IS NOT NULL AND success = 1 \
             GROUP BY model ORDER BY total_cost DESC LIMIT 5"
        ).map_err(|e| e.to_string())?;
        let rows2 = stmt2.query_map([], |row| {
            let model: String = row.get(0)?;
            let cost: f64 = row.get(1)?;
            let cnt: i64 = row.get(2)?;
            Ok(json!({
                "type": "high_cost",
                "severity": "medium",
                "model": model,
                "cost": cost,
                "requests": cnt,
                "message": format!("模型 {} 累计花费 ${:.2}（{} 次请求），考虑使用更经济的替代模型", model, cost, cnt),
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows2 { suggestions.push(row.map_err(|e| e.to_string())?); }

        // 3. 高延迟模型
        let mut stmt3 = conn.prepare(
            "SELECT model, AVG(latency_ms) as avg_lat, COUNT(*) as cnt \
             FROM usage_history WHERE model IS NOT NULL AND success = 1 AND latency_ms IS NOT NULL \
             GROUP BY model HAVING cnt >= 5 ORDER BY avg_lat DESC LIMIT 5"
        ).map_err(|e| e.to_string())?;
        let rows3 = stmt3.query_map([], |row| {
            let model: String = row.get(0)?;
            let lat: f64 = row.get(1)?;
            Ok(json!({
                "type": "high_latency",
                "severity": "low",
                "model": model,
                "avgLatencyMs": lat,
                "message": format!("模型 {} 平均延迟 {:.0}ms，考虑切换到更快的模型或连接", model, lat),
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows3 { suggestions.push(row.map_err(|e| e.to_string())?); }

        // 4. 压缩节省效果
        let total_saved: i64 = conn.query_row(
            "SELECT COALESCE(SUM(saved_tokens), 0) FROM usage_history", [], |row| row.get(0)
        ).unwrap_or(0);
        if total_saved > 0 {
            suggestions.push(json!({
                "type": "compression_savings",
                "severity": "info",
                "savedTokens": total_saved,
                "message": format!("Prompt 压缩已累计节省 ~{} tokens", total_saved),
            }));
        }

        // 5. 速率限制触发频繁的连接（通过 error_code 匹配 CircuitOpen/rate-limited）
        let mut stmt4 = conn.prepare(
            "SELECT provider, COUNT(*) as cnt \
             FROM usage_history WHERE success = 0 AND error_code LIKE '%rate-limited%' \
             GROUP BY provider HAVING cnt >= 3 ORDER BY cnt DESC LIMIT 3"
        ).map_err(|e| e.to_string())?;
        let rows4 = stmt4.query_map([], |row| {
            Ok(json!({
                "type": "rate_limited",
                "severity": "medium",
                "provider": row.get::<_, String>(0)?,
                "count": row.get::<_, i64>(1)?,
                "message": format!("提供方 {} 频繁触发速率限制，建议调整 RPM 或添加更多连接", row.get::<_, String>(0)?),
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows4 { suggestions.push(row.map_err(|e| e.to_string())?); }

        Ok(json!({ "suggestions": suggestions }))
    }).await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
