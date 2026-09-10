use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, providers as db_providers};
use crate::db::models::UsageEntry;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

pub async fn create_embeddings(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let model = body.get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let start = Instant::now();
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    if state.config.require_api_key {
        let _ = &conn;
    }

    let (provider_id, resolved_model) = if let Some((pid, _def, m)) = state.provider_registry.resolve_model_provider(model) {
        (pid, m)
    } else {
        return HttpResponse::NotFound().json(json!({
            "error": {"message": format!("Model '{}' not found", model), "type": "invalid_request_error"}
        }));
    };

    let def = match state.provider_registry.get(&provider_id) {
        Some(d) => d,
        None => return HttpResponse::NotFound().json(json!({"error": "Provider not found"})),
    };

    let connections = match db_providers::list_by_provider(&conn, &provider_id, &state.encryption_key) {
        Ok(c) => c,
        Err(_) => vec![],
    };

    let connection = match connections.into_iter().next() {
        Some(c) => c,
        None => {
            if !def.no_auth {
                return HttpResponse::NotFound().json(json!({"error": "No active connection for this provider"}));
            }
            return HttpResponse::ServiceUnavailable().json(json!({"error": "No connection available"}));
        }
    };

    let client = &state.http_client;
    let base_url = connection.provider_specific_data.get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or(&def.base_url);

    let url = format!("{}/v1/embeddings", base_url);

    let mut req_body = body.into_inner();
    if let Some(obj) = req_body.as_object_mut() {
        obj.insert("model".to_string(), json!(resolved_model));
    }

    let mut req_builder = client.post(&url);
    if let Some(ref api_key) = connection.api_key {
        req_builder = req_builder.bearer_auth(api_key);
    }

    match req_builder.json(&req_body).send().await {
        Ok(response) => {
            let status = response.status();
            let body_bytes = response.bytes().await.unwrap_or_default();

            if !status.is_success() {
                return HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY))
                    .body(body_bytes.to_vec());
            }

            let latency = start.elapsed().as_millis() as i64;
            let entry = UsageEntry {
                id: 0, provider: Some(provider_id), model: Some(resolved_model),
                connection_id: Some(connection.id.clone()), api_key_id: None, api_key_name: None,
                tokens_input: 0, tokens_output: 0, tokens_cache_read: 0,
                tokens_cache_creation: 0, tokens_reasoning: 0,
                service_tier: "standard".to_string(), status: "success".to_string(),
                success: true, error_code: None, latency_ms: Some(latency), ttft_ms: None,
                cost: 0.0, timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = db_core::get_conn(&state.db_pool).ok().and_then(|c| db_usage::record(&c, &entry).ok());

            HttpResponse::Ok()
                .content_type("application/json")
                .body(body_bytes.to_vec())
        }
        Err(e) => {
            HttpResponse::BadGateway().json(json!({"error": format!("Provider request failed: {}", e)}))
        }
    }
}

use crate::db::usage as db_usage;
