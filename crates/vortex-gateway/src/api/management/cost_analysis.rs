//! 成本分析端点：按模型/提供商/时间维度分析 API 支出。
use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct CostQuery {
    #[serde(default)]
    pub since: Option<String>,
}

pub async fn cost_analysis(
    state: web::Data<std::sync::Arc<AppState>>,
    query: web::Query<CostQuery>,
) -> HttpResponse {
    let pool = state.db_pool.clone();
    let since = query.since.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        let where_clause = since.as_ref().map(|s| format!("WHERE timestamp >= '{}'", s.replace('\'', "''"))).unwrap_or_default();

        let total_cost: f64 = conn.query_row(
            &format!("SELECT COALESCE(SUM(cost), 0.0) FROM usage_history {}", where_clause), [], |row| row.get(0)
        ).unwrap_or(0.0);

        let total_saved: i64 = conn.query_row(
            &format!("SELECT COALESCE(SUM(saved_tokens), 0) FROM usage_history {}", where_clause), [], |row| row.get(0)
        ).unwrap_or(0);

        let total_requests: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM usage_history {}", where_clause), [], |row| row.get(0)
        ).unwrap_or(0);

        let mut by_model = Vec::new();
        // 按「供应方 + 模型」分组，同名模型在不同供应方下分行展示；
        // 按使用量（输入+输出 Tokens）倒序，使用最多的模型排在最前；成本作次级排序
        let sql = format!(
            "SELECT provider, model, COUNT(*) as cnt, COALESCE(SUM(cost), 0) as cost, \
             COALESCE(SUM(tokens_input), 0) as tin, COALESCE(SUM(tokens_output), 0) as tout, \
             COALESCE(SUM(saved_tokens), 0) as saved \
             FROM usage_history {} GROUP BY provider, model \
             ORDER BY (COALESCE(SUM(tokens_input), 0) + COALESCE(SUM(tokens_output), 0)) DESC, cost DESC",
            where_clause
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok(json!({
                "provider": row.get::<_, Option<String>>(0)?,
                "model": row.get::<_, Option<String>>(1)?,
                "requests": row.get::<_, i64>(2)?,
                "cost": row.get::<_, f64>(3)?,
                "tokensInput": row.get::<_, i64>(4)?,
                "tokensOutput": row.get::<_, i64>(5)?,
                "savedTokens": row.get::<_, i64>(6)?,
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows { by_model.push(row.map_err(|e| e.to_string())?); }

        let mut by_provider = Vec::new();
        let sql2 = format!(
            "SELECT provider, COUNT(*) as cnt, COALESCE(SUM(cost), 0) as cost \
             FROM usage_history {} GROUP BY provider ORDER BY cost DESC",
            where_clause
        );
        let mut stmt2 = conn.prepare(&sql2).map_err(|e| e.to_string())?;
        let rows2 = stmt2.query_map([], |row| {
            Ok(json!({
                "provider": row.get::<_, Option<String>>(0)?,
                "requests": row.get::<_, i64>(1)?,
                "cost": row.get::<_, f64>(2)?,
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows2 { by_provider.push(row.map_err(|e| e.to_string())?); }

        let mut by_day = Vec::new();
        let sql3 = format!(
            "SELECT DATE(timestamp) as day, COUNT(*) as cnt, COALESCE(SUM(cost), 0) as cost, \
             COALESCE(SUM(saved_tokens), 0) as saved \
             FROM usage_history {} GROUP BY DATE(timestamp) ORDER BY day DESC LIMIT 30",
            where_clause
        );
        let mut stmt3 = conn.prepare(&sql3).map_err(|e| e.to_string())?;
        let rows3 = stmt3.query_map([], |row| {
            Ok(json!({
                "day": row.get::<_, String>(0)?,
                "requests": row.get::<_, i64>(1)?,
                "cost": row.get::<_, f64>(2)?,
                "savedTokens": row.get::<_, i64>(3)?,
            }))
        }).map_err(|e| e.to_string())?;
        for row in rows3 { by_day.push(row.map_err(|e| e.to_string())?); }

        Ok(json!({
            "totalCost": total_cost,
            "totalSavedTokens": total_saved,
            "totalRequests": total_requests,
            "byModel": by_model,
            "byProvider": by_provider,
            "byDay": by_day,
        }))
    }).await;
    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
