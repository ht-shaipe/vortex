use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, providers as db_providers};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn list_models(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let mut models = vec![];

    for def in state.provider_registry.list() {
        let connections = db_providers::list_by_provider(&conn, &def.id, &state.encryption_key).unwrap_or_default();
        if !connections.is_empty() || def.no_auth {
            match fetch_provider_models(&state, &def.id, &def, &connections).await {
                Ok(provider_models) => {
                    for m in provider_models {
                        models.push(m);
                    }
                }
                Err(_) => {
                    models.push(json!({
                        "id": format!("{}/models", def.id),
                        "object": "model",
                        "created": 0,
                        "owned_by": def.id,
                        "permission": [],
                    }));
                }
            }
        }
    }

    HttpResponse::Ok().json(json!({
        "object": "list",
        "data": models,
    }))
}

async fn fetch_provider_models(
    state: &Arc<AppState>,
    provider_id: &str,
    def: &crate::providers::types::ProviderDef,
    connections: &[crate::db::models::ProviderConnection],
) -> Result<Vec<serde_json::Value>, String> {
    let client = &state.http_client;
    let base_url = connections.first()
        .and_then(|c| c.provider_specific_data.get("baseUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or(&def.base_url);

    let url = format!("{}{}", base_url, def.models_path);

    let mut req_builder = client.get(&url).timeout(std::time::Duration::from_secs(10));

    if def.api_format == "anthropic" {
        if let Some(conn) = connections.first() {
            if let Some(ref api_key) = conn.api_key {
                req_builder = req_builder.header("x-api-key", api_key.as_str());
            }
        }
        req_builder = req_builder.header("anthropic-version", "2023-06-01");
    } else if def.api_format == "gemini" {
        if let Some(conn) = connections.first() {
            if let Some(ref api_key) = conn.api_key {
                req_builder = req_builder.query(&[("key", api_key.as_str())]);
            }
        }
    } else {
        if let Some(conn) = connections.first() {
            if let Some(ref api_key) = conn.api_key {
                req_builder = req_builder.bearer_auth(api_key.as_str());
            }
        }
    }

    let response = req_builder
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Provider returned {}", response.status()));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| format!("Parse error: {}", e))?;

    let model_ids = match def.api_format.as_str() {
        "anthropic" => {
            body.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "gemini" => {
            body.get("models")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("name").and_then(|v| v.as_str()).map(|s| format!("gemini/{}", s.split('/').last().unwrap_or(s))))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        _ => {
            body.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            if id.is_empty() { None } else { Some(format!("{}/{}", provider_id, id)) }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
    };

    let mut result = vec![];
    for model_id in model_ids {
        result.push(json!({
            "id": model_id,
            "object": "model",
            "created": chrono::Utc::now().timestamp(),
            "owned_by": provider_id,
            "permission": [],
        }));
    }

    Ok(result)
}
