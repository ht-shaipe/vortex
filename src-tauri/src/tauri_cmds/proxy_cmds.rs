//! 代理启停相关 Tauri 命令。
//!
//! 薄包装层，业务逻辑委托给 [`vortex_gateway::services::proxy_service`]。

use std::sync::Arc;

use vortex_gateway::AppState;
use vortex_gateway::services::proxy_service;

#[tauri::command]
pub async fn start_proxy(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    proxy_service::start_proxy(state.inner())
}

#[tauri::command]
pub async fn stop_proxy(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    proxy_service::stop_proxy(state.inner()).await
}
