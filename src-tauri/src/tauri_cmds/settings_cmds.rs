//! 应用设置相关 Tauri 命令。
//!
//! 薄包装层，业务逻辑委托给 [`vortex_gateway::services::settings_service`]。

use std::sync::Arc;

use vortex_gateway::AppState;
use vortex_gateway::services::settings_service;

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    settings_service::get_settings(&state)
}

#[tauri::command]
pub async fn update_settings(
    state: tauri::State<'_, Arc<AppState>>,
    updates: serde_json::Value,
) -> Result<serde_json::Value, String> {
    settings_service::update_settings(&state, &updates)
}
