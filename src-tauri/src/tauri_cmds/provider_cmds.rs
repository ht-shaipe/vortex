use crate::db::{core as db_core, providers as db_providers};
use crate::db::models::CreateProviderRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

#[tauri::command]
pub async fn list_providers(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let connections = db_providers::list(&conn, &state.encryption_key).map_err(|e| e.to_string())?;
    let registry = state.provider_registry.list();
    let mut result = vec![];
    for def in registry {
        let provider_connections: Vec<_> = connections.iter()
            .filter(|c| c.provider == def.id)
            .collect();
        result.push(json!({
            "id": def.id, "name": def.name, "icon": def.icon, "color": def.color,
            "hasFree": def.has_free, "noAuth": def.no_auth,
            "connections": provider_connections.len(),
        }));
    }

    let masked_connections: Vec<_> = connections.iter().map(|c| {
        let masked_key = c.api_key.as_deref().map(|k| {
            if k.len() > 8 { format!("{}{}", "*".repeat(k.len()-4), &k[k.len()-4..]) } else { "*".repeat(k.len()) }
        });
        json!({
            "id": c.id, "provider": c.provider, "name": c.name, "isActive": c.is_active,
            "apiKey": masked_key, "testStatus": c.test_status,
        })
    }).collect();
    Ok(json!({"providers": result, "connections": masked_connections}))
}

#[tauri::command]
pub async fn add_provider(
    state: tauri::State<'_, Arc<AppState>>,
    provider: String,
    name: String,
    api_key: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let req = CreateProviderRequest {
        provider,
        auth_type: None,
        name,
        email: None,
        api_key,
        access_token: None,
        refresh_token: None,
        project_id: None,
        base_url: None,
        priority: None,
        group_name: None,
        max_concurrent: None,
        default_model: None,
        provider_specific_data: None,
    };
    let created = db_providers::create(&conn, &req, &state.encryption_key).map_err(|e| e.to_string())?;
    let masked_key = created.api_key.as_deref().map(|k| {
        if k.len() > 8 { format!("{}{}", "*".repeat(k.len()-4), &k[k.len()-4..]) } else { "*".repeat(k.len()) }
    });
    Ok(json!({
        "id": created.id, "provider": created.provider, "name": created.name,
        "apiKey": masked_key, "isActive": created.is_active,
    }))
}

#[tauri::command]
pub async fn test_provider(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let provider = db_providers::get_by_id(&conn, &id, &state.encryption_key).map_err(|e| e.to_string())?;
    match provider {
        Some(p) => {
            let def = state.provider_registry.get(&p.provider);
            match def {
                Some(d) => {
                    let base_url = p.provider_specific_data.get("baseUrl")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&d.base_url);
                    let url = format!("{}{}", base_url, d.models_path);

                    let client = &state.http_client;
                    let mut req = client.get(&url).timeout(std::time::Duration::from_secs(10));

                    if d.api_format == "anthropic" {
                        if let Some(ref api_key) = p.api_key {
                            req = req.header("x-api-key", api_key.as_str());
                        }
                        req = req.header("anthropic-version", "2023-06-01");
                    } else if d.api_format == "gemini" {
                        if let Some(ref api_key) = p.api_key {
                            req = req.query(&[("key", api_key.as_str())]);
                        }
                    } else if let Some(ref api_key) = p.api_key {
                        req = req.bearer_auth(api_key.as_str());
                    }

                    match req.send().await {
                        Ok(response) => {
                            let status = response.status().as_u16();
                            let ok = status < 400;
                            let status_msg = if ok { None } else { Some(format!("HTTP {}", status)) };
                            let _ = db_providers::update_test_status(&conn, &id,
                                if ok { "ok" } else { "error" },
                                status_msg.as_deref()
                            );
                            Ok(json!({
                                "status": if ok { "ok" } else { "error" },
                                "statusCode": status,
                                "provider": p.name,
                            }))
                        }
                        Err(e) => {
                            let _ = db_providers::update_test_status(&conn, &id, "error", Some(&e.to_string()));
                            Ok(json!({
                                "status": "error",
                                "error": e.to_string(),
                                "provider": p.name,
                            }))
                        }
                    }
                }
                None => Ok(json!({"status": "error", "error": "Provider definition not found"})),
            }
        }
        None => Err("Provider not found".into()),
    }
}
