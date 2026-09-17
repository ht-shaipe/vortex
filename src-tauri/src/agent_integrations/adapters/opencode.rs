//! OpenCode 适配器
//!
//! 负责检测和配置 OpenCode 智能体。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent_integrations::{
    AgentAdapter, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    FileChange, FileOperation, RestoreResult, VortexConfigStatus, VortexGatewayConfig,
};
use crate::agent_integrations::backup::BackupManager;
use crate::agent_integrations::detect;
use crate::agent_integrations::safe_write;

/// OpenCode 适配器
pub struct OpenCodeAdapter;

impl OpenCodeAdapter {
    /// 创建新的适配器
    pub fn new() -> Self {
        Self
    }

    /// 获取 OpenCode 配置目录
    fn get_opencode_home(home: &Path) -> PathBuf {
        detect::resolve_agent_home("OPENCODE_HOME", ".config/opencode")
            .unwrap_or_else(|| home.join(".config/opencode"))
    }

    /// 检查 Vortex 提供方是否已配置
    fn check_vortex_configured(settings_path: &Path) -> Result<bool, String> {
        if !settings_path.exists() {
            return Ok(false);
        }

        let content = std::fs::read_to_string(settings_path)
            .map_err(|e| format!("读取设置文件失败: {}", e))?;

        let json: Value = serde_json::from_str(&content)
            .map_err(|e| format!("解析 JSON 失败: {}", e))?;

        // 检查 providers 对象中是否有 vortex
        if let Some(providers) = json.get("providers").and_then(|p| p.as_object()) {
            return Ok(providers.contains_key("vortex"));
        }

        Ok(false)
    }

    /// 生成 Vortex 提供方配置
    fn generate_provider_config(config: &VortexGatewayConfig) -> Value {
        let mut models = serde_json::Map::new();

        for model in &config.models {
            models.insert(
                model.id.clone(),
                serde_json::json!({
                    "name": model.name,
                    "contextWindow": model.context_window.unwrap_or(200000)
                }),
            );
        }

        serde_json::json!({
            "vortex": {
                "npm": "@vortex-ai/opencode-provider",
                "options": {
                    "baseURL": &config.openai_base_url,
                    "apiKeyEnv": "VORTEX_API_KEY",
                    "models": models
                }
            }
        })
    }
}

impl AgentAdapter for OpenCodeAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::OpenCode
    }

    fn detect(&self, home: &PathBuf) -> Result<DetectedAgent, String> {
        // 检测 opencode 可执行文件
        let install_path = detect::find_in_path("opencode");
        let installed = install_path.is_some();

        // 获取版本
        let version = install_path
            .as_ref()
            .and_then(|p| detect::get_version(p, &["--version"]));

        // 检查配置文件
        let opencode_home = Self::get_opencode_home(home);
        let settings_path = opencode_home.join("opencode.json");
        let config_exists = settings_path.exists();

        // 检查 Vortex 配置状态
        let vortex_status = if config_exists {
            match Self::check_vortex_configured(&settings_path) {
                Ok(true) => VortexConfigStatus::Configured,
                Ok(false) => VortexConfigStatus::NotConfigured,
                Err(_) => VortexConfigStatus::Failed,
            }
        } else {
            VortexConfigStatus::NotConfigured
        };

        Ok(DetectedAgent {
            kind: AgentKind::OpenCode,
            installed,
            install_path,
            version,
            config_exists,
            config_path: if config_exists { Some(settings_path) } else { None },
            supports_auto_config: true,
            unsupported_reason: None,
            vortex_status,
        })
    }

    fn preview(
        &self,
        home: &PathBuf,
        config: &VortexGatewayConfig,
    ) -> Result<ConfigPreview, String> {
        let opencode_home = Self::get_opencode_home(home);
        let settings_path = opencode_home.join("opencode.json");

        let mut files_to_modify = Vec::new();
        let mut warnings = Vec::new();

        // 检查 opencode.json
        if settings_path.exists() {
            files_to_modify.push(FileChange {
                path: settings_path.clone(),
                operation: FileOperation::Modify,
                summary: "添加 Vortex 提供方配置".to_string(),
            });
        } else {
            files_to_modify.push(FileChange {
                path: settings_path,
                operation: FileOperation::Create,
                summary: "创建设置文件并添加 Vortex 提供方".to_string(),
            });
        }

        warnings.push("请确保已设置环境变量 VORTEX_API_KEY".to_string());
        warnings.push("配置完成后需要重启 OpenCode 才能生效".to_string());

        Ok(ConfigPreview {
            agent: AgentKind::OpenCode,
            files_to_modify,
            requires_restart: true,
            warnings,
        })
    }

    fn apply(
        &self,
        home: &PathBuf,
        config: &VortexGatewayConfig,
        backup_dir: &PathBuf,
    ) -> Result<ConfigResult, String> {
        let opencode_home = Self::get_opencode_home(home);
        let settings_path = opencode_home.join("opencode.json");

        // 确保 OpenCode 目录存在
        std::fs::create_dir_all(&opencode_home)
            .map_err(|e| format!("创建 OpenCode 目录失败: {}", e))?;

        // 创建备份管理器
        let backup_manager = BackupManager::new(backup_dir.clone());

        // 准备要备份的文件
        let mut files_to_backup = Vec::new();
        if settings_path.exists() {
            files_to_backup.push((settings_path.clone(), None));
        }

        // 创建备份
        let manifest = backup_manager.create_backup(AgentKind::OpenCode, &files_to_backup)?;

        // 读取或创建现有配置
        let mut settings: Value = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)
                .map_err(|e| format!("读取现有设置失败: {}", e))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("解析 JSON 失败: {}", e))?
        } else {
            serde_json::json!({})
        };

        // 生成 Vortex 提供方配置
        let vortex_provider = Self::generate_provider_config(config);

        // 添加或更新 providers 对象
        if let Some(providers) = settings.get_mut("providers").and_then(|p| p.as_object_mut()) {
            // 合并 vortex 配置
            if let Some(vortex_obj) = vortex_provider.as_object() {
                for (key, value) in vortex_obj {
                    providers.insert(key.clone(), value.clone());
                }
            }
        } else {
            // 创建 providers 对象
            settings["providers"] = vortex_provider;
        }

        // 写入配置文件
        let new_content = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("序列化 JSON 失败: {}", e))?;

        safe_write::atomic_write_with_backup(&settings_path, new_content.as_bytes(), backup_dir)?;

        Ok(ConfigResult {
            agent: AgentKind::OpenCode,
            success: true,
            error: None,
            backup_id: Some(manifest.id),
            requires_restart: true,
        })
    }

    fn restore(
        &self,
        home: &PathBuf,
        backup_id: &str,
        backup_dir: &PathBuf,
    ) -> Result<RestoreResult, String> {
        let backup_manager = BackupManager::new(backup_dir.clone());
        let manifest = backup_manager.load_manifest(backup_id)?;

        if !backup_manager.verify_backup(backup_id)? {
            return Err("备份文件不完整或已损坏".to_string());
        }

        for file in &manifest.files {
            if file.existed {
                std::fs::copy(&file.backup_path, &file.original_path)
                    .map_err(|e| format!("恢复文件失败 {}: {}", file.original_path.display(), e))?;
            } else if file.original_path.exists() {
                std::fs::remove_file(&file.original_path)
                    .map_err(|e| format!("删除文件失败 {}: {}", file.original_path.display(), e))?;
            }
        }

        Ok(RestoreResult {
            agent: AgentKind::OpenCode,
            success: true,
            error: None,
        })
    }
}
