//! 智能体集成 Tauri 命令。
//!
//! 薄包装层，业务逻辑委托给 [`vortex_gateway::services::agent_service`]。

use std::sync::Arc;

use tauri::State;

use vortex_gateway::AppState;
use vortex_gateway::agent_integrations::AgentKind;
use vortex_gateway::services::agent_service::{
    self, ApplyConfigRequest, ApplyConfigResponse, DetectAgentsResponse,
    ListBackupsResponse, PreviewConfigResponse, RestoreConfigRequest, RestoreConfigResponse,
};

/// 检测本机已安装且可配置的 AI 编程智能体。
#[tauri::command]
pub async fn agent_detect() -> Result<DetectAgentsResponse, String> {
    agent_service::detect_agents()
}

/// 预览某个智能体的配置变更（不落盘）。
#[tauri::command]
pub async fn agent_preview_config(
    state: State<'_, Arc<AppState>>,
    agent: AgentKind,
) -> Result<PreviewConfigResponse, String> {
    agent_service::preview_config(&state, agent)
}

/// 应用配置变更，返回实际写入的文件操作结果。
#[tauri::command]
pub async fn agent_apply_config(
    state: State<'_, Arc<AppState>>,
    request: ApplyConfigRequest,
) -> Result<ApplyConfigResponse, String> {
    agent_service::apply_config(&state, request)
}

/// 从备份恢复智能体配置。
#[tauri::command]
pub async fn agent_restore_config(
    request: RestoreConfigRequest,
) -> Result<RestoreConfigResponse, String> {
    agent_service::restore_config(request)
}

/// 列出可用的配置备份。
#[tauri::command]
pub async fn agent_list_backups() -> Result<ListBackupsResponse, String> {
    agent_service::list_backups()
}
