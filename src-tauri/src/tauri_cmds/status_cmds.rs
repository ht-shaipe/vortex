//! 系统运行状态相关 Tauri 命令。
//!
//! 薄包装层，业务逻辑委托给 [`vortex_gateway::services::system_service`]。

use std::sync::Arc;

use vortex_gateway::AppState;
use vortex_gateway::services::system_service::SystemStatus;
use vortex_gateway::services::system_service;

#[tauri::command]
pub async fn get_system_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SystemStatus, String> {
    system_service::get_system_status(&state)
}
