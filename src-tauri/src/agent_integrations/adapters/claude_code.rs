//! Claude Code 适配器
//!
//! 负责检测和配置 Claude Code 智能体。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent_integrations::{
    AgentAdapter, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    FileChange, FileOperation, RestoreResult, VortexConfigStatus, VortexGatewayConfig,
};
use crate::agent_integrations::backup::BackupManager;
use crate::agent_integrations::detect;
use crate::agent_integrations::safe_write;

/// Claude Code 适配器
pub struct ClaudeCodeAdapter;

impl ClaudeCodeAdapter {
    /// 创建新的适配器
    pub fn new() -> Self {
        Self
    }

    /// 获取 Claude Code 配置目录
    fn get_claude_home(home: &Path) -> PathBuf {
        detect::resolve_agent_home("CLAUDE_HOME", ".claude").unwrap_or_else(|| home.join(".claude"))
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

        // 检查是否有 Vortex 提供方
        if let Some(providers) = json.get("providers").and_then(|p| p.as_array()) {
            return Ok(providers.iter().any(|p| {
                p.get("name").and_then(|n| n.as_str()) == Some("Vortex")
            }));
        }

        Ok(false)
    }

    /// 生成 Vortex 提供方配置
    fn generate_provider_config(config: &VortexGatewayConfig) -> Value {
        serde_json::json!({
            "name": "Vortex",
            "baseURL": &config.openai_base_url,
            "apiKeyEnv": "VORTEX_API_KEY",
            "models": config.models.iter().map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "contextWindow": m.context_window.unwrap_or(200000)
                })
            }).collect::<Vec<_>>()
        })
    }
}

impl AgentAdapter for ClaudeCodeAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::ClaudeCode
    }

    fn detect(&self, home: &Path) -> Result<DetectedAgent, String> {
        // 检测 claude 可执行文件
        let install_path = detect::find_in_path("claude");
        let installed = install_path.is_some();

        // 获取版本
        let version = install_path
            .as_ref()
            .and_then(|p| detect::get_version(p, &["--version"]));

        // 检查配置文件
        let claude_home = Self::get_claude_home(home);
        let settings_path = claude_home.join("settings.json");
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
            kind: AgentKind::ClaudeCode,
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
        home: &Path,
        _config: &VortexGatewayConfig,
    ) -> Result<ConfigPreview, String> {
        let claude_home = Self::get_claude_home(home);
        let settings_path = claude_home.join("settings.json");

        let mut files_to_modify = Vec::new();
        let mut warnings = Vec::new();

        // 检查 settings.json
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

        // Claude Code 会自动检测环境变量变量，VORTEX_API_KEY 将自动注入到 env 部分
        warnings.push("VORTEX_API_KEY 将自动写入 settings.json 的 env 部分".to_string());
        warnings.push("配置完成后需要重启 Claude Code 才能生效".to_string());

        Ok(ConfigPreview {
            agent: AgentKind::ClaudeCode,
            files_to_modify,
            requires_restart: true,
            warnings,
        })
    }

    fn apply(
        &self,
        home: &Path,
        config: &VortexGatewayConfig,
        backup_dir: &Path,
    ) -> Result<ConfigResult, String> {
        let claude_home = Self::get_claude_home(home);
        let settings_path = claude_home.join("settings.json");

        // 确保 Claude 目录存在
        std::fs::create_dir_all(&claude_home)
            .map_err(|e| format!("创建 Claude 目录失败: {}", e))?;

        // 创建备份管理器
        let backup_manager = BackupManager::new(backup_dir);

        // 准备要备份的文件
        let mut files_to_backup = Vec::new();
        if settings_path.exists() {
            files_to_backup.push((settings_path.clone(), None));
        }

        // 创建备份
        let manifest = backup_manager.create_backup(AgentKind::ClaudeCode, &files_to_backup)?;

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

        // 添加或更新 providers 数组
        if let Some(providers) = settings.get_mut("providers").and_then(|p| p.as_array_mut()) {
            // 检查是否已有 Vortex 提供方
            let existing_idx = providers.iter().position(|p| {
                p.get("name").and_then(|n| n.as_str()) == Some("Vortex")
            });

            if let Some(idx) = existing_idx {
                // 更新现有配置
                providers[idx] = vortex_provider;
            } else {
                // 添加新配置
                providers.push(vortex_provider);
            }
        } else {
            // 创建 providers 数组
            settings["providers"] = serde_json::json!([vortex_provider]);
        }

        // 自动注入 VORTEX_API_KEY 到 env 部分，确保 Claude Code 能认证
        if let Some(env) = settings.get_mut("env").and_then(|e| e.as_object_mut()) {
            env.insert("VORTEX_API_KEY".to_string(), serde_json::json!(&config.token));
        } else {
            settings["env"] = serde_json::json!({
                "VORTEX_API_KEY": &config.token
            });
        }

        // 写入配置文件
        let new_content = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("序列化 JSON 失败: {}", e))?;

        safe_write::atomic_write_with_backup(&settings_path, new_content.as_bytes(), backup_dir)?;

        Ok(ConfigResult {
            agent: AgentKind::ClaudeCode,
            success: true,
            error: None,
            backup_id: Some(manifest.id),
            requires_restart: true,
        })
    }

    fn restore(
        &self,
        _home: &Path,
        backup_id: &str,
        backup_dir: &Path,
    ) -> Result<RestoreResult, String> {
        let backup_manager = BackupManager::new(backup_dir);
        let manifest = backup_manager.load_manifest(backup_id)?;

        // 验证备份完整性
        if !backup_manager.verify_backup(backup_id)? {
            return Err("备份文件不完整或已损坏".to_string());
        }

        // 恢复每个文件
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
            agent: AgentKind::ClaudeCode,
            success: true,
            error: None,
        })
    }
}
