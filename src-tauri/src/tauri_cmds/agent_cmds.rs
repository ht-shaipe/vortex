//! 智能体集成 IPC 命令
//!
//! 提供前端与智能体集成模块交互的 Tauri 命令。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use vortex_gateway::agent_integrations::{
    AdapterRegistry, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    RestoreResult, VortexGatewayConfig,
};
use vortex_gateway::agent_integrations::backup::BackupManager;
use vortex_gateway::agent_integrations::detect;
use vortex_gateway::agent_integrations::ModelInfo;
use vortex_store::db::{core as db_core, model_aliases as db_aliases, providers as db_providers, settings as db_settings};
use vortex_gateway::AppState;

/// 智能体检测响应
#[derive(Debug, Serialize, Deserialize)]
pub struct DetectAgentsResponse {
    /// 检测到的智能体列表
    pub agents: Vec<DetectedAgent>,
    /// 检测耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 配置预览响应
#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewConfigResponse {
    /// 配置预览
    pub preview: ConfigPreview,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// 应用配置请求
#[derive(Debug, Deserialize)]
pub struct ApplyConfigRequest {
    /// 智能体类型
    pub agent: AgentKind,
    /// 确认应用
    pub confirmed: bool,
}

/// 应用配置响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ApplyConfigResponse {
    /// 配置结果
    pub result: ConfigResult,
}

/// 恢复配置请求
#[derive(Debug, Deserialize)]
pub struct RestoreConfigRequest {
    /// 智能体类型
    pub agent: AgentKind,
    /// 备份 ID
    pub backup_id: String,
}

/// 恢复配置响应
#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreConfigResponse {
    /// 恢复结果
    pub result: RestoreResult,
}

/// 备份列表响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ListBackupsResponse {
    /// 备份列表
    pub backups: Vec<BackupInfo>,
}

/// 备份信息
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupInfo {
    /// 备份 ID
    pub id: String,
    /// 智能体类型
    pub agent: AgentKind,
    /// 创建时间
    pub created_at: String,
    /// 文件数量
    pub file_count: usize,
}

/// 检测所有智能体
#[tauri::command]
pub async fn agent_detect() -> Result<DetectAgentsResponse, String> {
    let start = std::time::Instant::now();

    let home = detect::get_home_dir()
        .ok_or_else(|| "无法获取用户主目录".to_string())?;

    let registry = AdapterRegistry::new();
    let agents = registry.detect_all(&home);

    Ok(DetectAgentsResponse {
        agents,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// 预览配置变更
#[tauri::command]
pub async fn agent_preview_config(
    state: State<'_, Arc<AppState>>,
    agent: AgentKind,
) -> Result<PreviewConfigResponse, String> {
    let home = detect::get_home_dir()
        .ok_or_else(|| "无法获取用户主目录".to_string())?;

    // 获取网关配置
    let gateway_config = get_gateway_config(&state).await?;

    let registry = AdapterRegistry::new();
    let adapter = registry
        .get(agent)
        .ok_or_else(|| format!("不支持的智能体类型: {:?}", agent))?;

    let preview = adapter.preview(&home, &gateway_config)?;

    Ok(PreviewConfigResponse {
        preview,
        warnings: Vec::new(),
    })
}

/// 应用配置
#[tauri::command]
pub async fn agent_apply_config(
    state: State<'_, Arc<AppState>>,
    request: ApplyConfigRequest,
) -> Result<ApplyConfigResponse, String> {
    if !request.confirmed {
        return Err("请先确认配置变更".to_string());
    }

    let home = detect::get_home_dir()
        .ok_or_else(|| "无法获取用户主目录".to_string())?;

    // 获取网关配置
    let gateway_config = get_gateway_config(&state).await?;

    // 获取备份目录
    let backup_dir = get_backup_dir()?;

    let registry = AdapterRegistry::new();
    let adapter = registry
        .get(request.agent)
        .ok_or_else(|| format!("不支持的智能体类型: {:?}", request.agent))?;

    let result = adapter.apply(&home, &gateway_config, &backup_dir)?;

    Ok(ApplyConfigResponse { result })
}

/// 恢复配置
#[tauri::command]
pub async fn agent_restore_config(
    request: RestoreConfigRequest,
) -> Result<RestoreConfigResponse, String> {
    let home = detect::get_home_dir()
        .ok_or_else(|| "无法获取用户主目录".to_string())?;

    let backup_dir = get_backup_dir()?;

    let registry = AdapterRegistry::new();
    let adapter = registry
        .get(request.agent)
        .ok_or_else(|| format!("不支持的智能体类型: {:?}", request.agent))?;

    let result = adapter.restore(&home, &request.backup_id, &backup_dir)?;

    Ok(RestoreConfigResponse { result })
}

/// 列出备份
#[tauri::command]
pub async fn agent_list_backups() -> Result<ListBackupsResponse, String> {
    let backup_dir = get_backup_dir()?;
    let manager = BackupManager::new(backup_dir);

    let manifests = manager.list_backups()?;

    let backups = manifests
        .into_iter()
        .map(|m| BackupInfo {
            id: m.id,
            agent: m.agent,
            created_at: m.created_at,
            file_count: m.files.len(),
        })
        .collect();

    Ok(ListBackupsResponse { backups })
}

/// 获取网关配置
async fn get_gateway_config(state: &State<'_, Arc<AppState>>) -> Result<VortexGatewayConfig, String> {
    // 获取网关端口
    let port = state.proxy_port;

    // 获取访问令牌
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let settings = db_settings::get_settings(&conn).map_err(|e| format!("获取设置失败: {}", e))?;
    let token = settings
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default-token".to_string());

    // 获取可用模型列表
    let models = get_available_models(state).await?;

    Ok(VortexGatewayConfig::new(port, token, models))
}

/// 获取可用模型列表（从数据库查询真实模型）
async fn get_available_models(state: &State<'_, Arc<AppState>>) -> Result<Vec<ModelInfo>, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 加载虚拟模型别名
    let aliases = db_aliases::list(&conn).unwrap_or_default();
    let active_aliases: Vec<_> = aliases.iter().filter(|a| a.is_active).collect();

    // 读取是否隐藏已映射的真实模型（默认 true）
    let hide_mapped = db_settings::get(&conn, "settings", "general")
        .ok()
        .flatten()
        .and_then(|v| v.get("hideMappedModels").cloned())
        .map(|v| v.as_bool().unwrap_or(true))
        .unwrap_or(true);

    let hide_all_real = hide_mapped && !active_aliases.is_empty();

    // 如果不隐藏真实模型，遍历所有活跃连接收集模型
    if !hide_all_real {
        for def in state.provider_registry.list() {
            let connections = db_providers::list_by_provider(&conn, &def.id, &state.encryption_key)
                .unwrap_or_default();
            for c in connections.iter().filter(|c| c.is_active) {
                let ids: Vec<String> = match &c.models {
                    Some(list) if !list.is_empty() => list.iter().map(|m| m.id.clone()).collect(),
                    _ => c.default_model.clone().map(|m| vec![m]).unwrap_or_default(),
                };
                for id in ids {
                    if id.is_empty() { continue; }
                    let full = format!("{}/{}", def.id, id);
                    if !seen.insert(full.clone()) { continue; }
                    // 尝试获取展示名
                    let display_name = c.models.as_ref()
                        .and_then(|ms| ms.iter().find(|m| m.id == id))
                        .and_then(|m| m.name.clone())
                        .unwrap_or_else(|| id.clone());
                    models.push(ModelInfo {
                        id: full,
                        name: display_name,
                        provider: def.id.clone(),
                        context_window: Some(200000),
                    });
                }
            }
        }
    }

    // 追加虚拟模型别名
    for alias in &active_aliases {
        if seen.insert(alias.alias.clone()) {
            models.push(ModelInfo {
                id: alias.alias.clone(),
                name: alias.alias.clone(),
                provider: "vortex".to_string(),
                context_window: Some(200000),
            });
        }
    }

    // 追加 auto 虚拟模型
    if seen.insert("auto".to_string()) {
        models.push(ModelInfo {
            id: "auto".to_string(),
            name: "Auto (智能选择)".to_string(),
            provider: "vortex".to_string(),
            context_window: Some(200000),
        });
    }

    Ok(models)
}

/// 获取备份目录
fn get_backup_dir() -> Result<PathBuf, String> {
    let home = detect::get_home_dir()
        .ok_or_else(|| "无法获取用户主目录".to_string())?;

    Ok(home.join(".vortex").join("agent_backups"))
}
