use actix_web::{web, HttpResponse};
use crate::db::{core as db_core, providers as db_providers};
use crate::db::models::CreateProviderRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

fn mask_connection(c: &crate::db::models::ProviderConnection) -> serde_json::Value {
    let masked_key = c.api_key.as_deref().map(|k| {
        if k.len() > 8 { format!("{}{}", "*".repeat(k.len()-4), &k[k.len()-4..]) } else { "*".repeat(k.len()) }
    });
    // 自定义提供方扩展字段：从 provider_specific_data 提取 baseUrl / apiProtocol / customId，
    // 这样前端不必理解 JSON 结构直接读平铺字段。
    let empty_obj = serde_json::json!({});
    let obj = c.provider_specific_data.as_object().unwrap_or(empty_obj.as_object().unwrap());
    let base_url = obj.get("baseUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
    let api_protocol = obj.get("apiProtocol").and_then(|v| v.as_str()).map(|s| s.to_string());
    let custom_id = obj.get("customId").and_then(|v| v.as_str()).map(|s| s.to_string());

    json!({
        "id": c.id,
        "provider": c.provider,
        "authType": c.auth_type,
        "name": c.name,
        "email": c.email,
        "priority": c.priority,
        "isActive": c.is_active,
        "apiKey": masked_key,
        "projectId": c.project_id,
        "testStatus": c.test_status,
        "errorCode": c.error_code,
        "lastError": c.last_error,
        "lastErrorAt": c.last_error_at,
        "backoffLevel": c.backoff_level,
        "rateLimitedUntil": c.rate_limited_until,
        "healthCheckInterval": c.health_check_interval,
        "consecutiveUseCount": c.consecutive_use_count,
        "rateLimitProtection": c.rate_limit_protection,
        "groupName": c.group_name,
        "maxConcurrent": c.max_concurrent,
        "proxyEnabled": c.proxy_enabled,
        "displayName": c.display_name,
        "defaultModel": c.default_model,
        "baseUrl": base_url,
        "apiProtocol": api_protocol,
        "customProviderId": custom_id,
        "providerSpecificData": c.provider_specific_data,
        "createdAt": c.created_at,
        "updatedAt": c.updated_at,
    })
}

pub async fn list_providers(
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let connections = db_providers::list(&conn, &state.encryption_key).unwrap_or_default();
    let registry = state.provider_registry.list();

    let mut result = vec![];
    for def in registry {
        let provider_connections: Vec<_> = connections.iter()
            .filter(|c| c.provider == def.id)
            .collect();
        result.push(json!({
            "id": def.id,
            "name": def.name,
            "icon": def.icon,
            "color": def.color,
            "hasFree": def.has_free,
            "noAuth": def.no_auth,
            "authHint": def.auth_hint,
            "serviceKinds": def.service_kinds,
            "connections": provider_connections.len(),
            "activeConnections": provider_connections.iter().filter(|c| c.is_active).count(),
        }));
    }

    let masked_connections: Vec<_> = connections.iter().map(|c| mask_connection(c)).collect();
    HttpResponse::Ok().json(json!({"providers": result, "connections": masked_connections}))
}

pub async fn create_provider(
    state: web::Data<Arc<AppState>>,
    body: web::Json<CreateProviderRequest>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    match db_providers::create(&conn, &body, &state.encryption_key) {
        Ok(provider) => HttpResponse::Created().json(mask_connection(&provider)),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn get_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_providers::get_by_id(&conn, &id, &state.encryption_key) {
        Ok(Some(provider)) => HttpResponse::Ok().json(mask_connection(&provider)),
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Provider not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn update_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_providers::update(&conn, &id, &body, &state.encryption_key) {
        Ok(Some(provider)) => HttpResponse::Ok().json(mask_connection(&provider)),
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Provider not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn delete_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    match db_providers::delete(&conn, &id) {
        Ok(true) => HttpResponse::Ok().json(json!({"success": true})),
        Ok(false) => HttpResponse::NotFound().json(json!({"error": "Provider not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

pub async fn test_provider(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let conn = match db_core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(e) => return HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    };

    let id = path.into_inner();
    let provider = db_providers::get_by_id(&conn, &id, &state.encryption_key);

    match provider {
        Ok(Some(p)) => {
            let def = state.provider_registry.get(&p.provider);
            let test_result = test_connection(&p, def).await;
            let _ = db_providers::update_test_status(&conn, &id, &test_result.status, test_result.error.as_deref());
            HttpResponse::Ok().json(json!({
                "status": test_result.status,
                "error": test_result.error,
                "latencyMs": test_result.latency_ms,
            }))
        }
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "Provider not found"})),
        Err(e) => HttpResponse::InternalServerError().json(json!({"error": e.to_string()})),
    }
}

struct TestResult {
    status: String,
    error: Option<String>,
    latency_ms: Option<u64>,
}

async fn test_connection(
    connection: &crate::db::models::ProviderConnection,
    provider_def: Option<&crate::providers::types::ProviderDef>,
) -> TestResult {
    let def = match provider_def {
        Some(d) => d,
        None => return TestResult { status: "error".into(), error: Some("Unknown provider".into()), latency_ms: None },
    };

    let base_url = connection.provider_specific_data.get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or(&def.base_url);

    let url = format!("{}{}", base_url, def.models_path);
    let client = reqwest::Client::new();

    let start = std::time::Instant::now();
    let mut req = client.get(&url);

    if let Some(ref api_key) = connection.api_key {
        if def.id == "anthropic" {
            req = req.header("x-api-key", api_key.as_str());
        } else if def.id == "gemini" {
            req = req.query(&[("key", api_key.as_str())]);
        } else {
            req = req.bearer_auth(api_key.as_str());
        }
    }

    match req.send().await {
        Ok(response) => {
            let latency = start.elapsed().as_millis() as u64;
            if response.status().is_success() {
                TestResult { status: "ok".into(), error: None, latency_ms: Some(latency) }
            } else {
                let status = response.status().as_u16();
                TestResult { status: "error".into(), error: Some(format!("HTTP {}", status)), latency_ms: Some(latency) }
            }
        }
        Err(e) => TestResult { status: "error".into(), error: Some(e.to_string()), latency_ms: None },
    }
}
