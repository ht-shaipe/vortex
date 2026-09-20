//! 提供商相关 Tauri 命令。
//!
//! 薄包装层，业务逻辑委托给 [`vortex_gateway::services::provider_service`]。

use std::sync::Arc;

use vortex_gateway::AppState;
use vortex_gateway::services::provider_service;

#[tauri::command]
pub async fn list_providers(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    provider_service::list_providers(&state)
}

#[tauri::command]
pub async fn add_provider(
    state: tauri::State<'_, Arc<AppState>>,
    provider: String,
    name: String,
    api_key: Option<String>,
) -> Result<serde_json::Value, String> {
    provider_service::add_provider(&state, provider, name, api_key)
}

#[tauri::command]
pub async fn test_provider(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
        rt.block_on(async { provider_service::test_provider(&app_state, &id).await })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}
