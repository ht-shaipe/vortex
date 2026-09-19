//! MCP 服务器端点：让 agent 自省可用模型、提供商健康、路由策略。

use actix_web::{web, HttpResponse};
use serde_json::json;
use crate::AppState;

pub async fn mcp_tools(_state: web::Data<std::sync::Arc<AppState>>) -> HttpResponse {
    let tools = json!({
        "tools": [
            {
                "name": "list_models",
                "description": "List all available models across providers",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_providers",
                "description": "List all configured providers and their health status",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_routing_strategy",
                "description": "Get current routing strategy and configuration",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_rate_limits",
                "description": "Get rate limit configuration and current usage",
                "inputSchema": { "type": "object", "properties": {} }
            }
        ]
    });
    HttpResponse::Ok().json(tools)
}

pub async fn mcp_call(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let tool_name = body.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let pool = state.db_pool.clone();
    let enc_key = state.encryption_key.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = vortex_store::db::core::get_conn(&pool).map_err(|e| e.to_string())?;
        match tool_name.as_str() {
            "list_models" => {
                let providers = vortex_store::db::providers::list_all_active(&conn, &enc_key).map_err(|e| e.to_string())?;
                let models: Vec<serde_json::Value> = providers.iter().flat_map(|p| {
                    p.models.as_ref().map(|ms| ms.iter().map(|m| json!({
                        "provider": p.provider,
                        "model": m.id,
                        "name": m.name.as_ref().unwrap_or(&m.id),
                        "connection": p.name,
                    })).collect::<Vec<_>>()).unwrap_or_default()
                }).collect();
                Ok(json!({ "models": models }))
            }
            "list_providers" => {
                let providers = vortex_store::db::providers::list_all_active(&conn, &enc_key).map_err(|e| e.to_string())?;
                let list: Vec<serde_json::Value> = providers.iter().map(|p| json!({
                    "provider": p.provider,
                    "name": p.name,
                    "priority": p.priority,
                    "testStatus": p.test_status,
                    "lastLatencyMs": p.last_latency_ms,
                })).collect();
                Ok(json!({ "providers": list }))
            }
            "get_rate_limits" => {
                let snapshots = vortex_store::db::rate_limits::list_usage_snapshots(&conn).map_err(|e| e.to_string())?;
                Ok(json!({ "snapshots": snapshots }))
            }
            _ => Ok(json!({ "error": format!("Unknown tool: {}", tool_name) })),
        }
    })
    .await;

    match result {
        Ok(Ok(v)) => HttpResponse::Ok().json(v),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e.to_string() })),
    }
}
