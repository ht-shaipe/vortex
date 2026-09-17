//! Codex 适配器
//!
//! 负责检测和配置 OpenAI Codex 智能体。

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, value, Item, Table};

use crate::agent_integrations::{
    AgentAdapter, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    FileChange, FileOperation, RestoreResult, VortexConfigStatus, VortexGatewayConfig,
};
use crate::agent_integrations::backup::BackupManager;
use crate::agent_integrations::detect;
use crate::agent_integrations::safe_write;

/// Codex 适配器
pub struct CodexAdapter;

impl CodexAdapter {
    /// 创建新的适配器
    pub fn new() -> Self {
        Self
    }

    /// 获取 Codex 配置目录
    fn get_codex_home(home: &Path) -> PathBuf {
        detect::resolve_agent_home("CODEX_HOME", ".codex").unwrap_or_else(|| home.join(".codex"))
    }

    /// 检查 Vortex 提供方是否已配置
    fn check_vortex_configured(config_path: &Path) -> Result<bool, String> {
        if !config_path.exists() {
            return Ok(false);
        }

        let content = std::fs::read_to_string(config_path)
            .map_err(|e| format!("读取配置文件失败: {}", e))?;

        let doc: DocumentMut = content.parse()
            .map_err(|e| format!("解析 TOML 失败: {}", e))?;

        // 检查是否有 [provider.vortex] 或 [provider.custom.vortex]
        if let Some(provider) = doc.get("provider")
            && let Some(provider_table) = provider.as_table() {
                // 检查直接子项
                if provider_table.contains_key("vortex") {
                    return Ok(true);
                }

                // 检查 custom 子表
                if let Some(custom) = provider_table.get("custom").and_then(|c| c.as_table())
                    && custom.contains_key("vortex") {
                        return Ok(true);
                    }
            }

        Ok(false)
    }
}

impl AgentAdapter for CodexAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn detect(&self, home: &Path) -> Result<DetectedAgent, String> {
        // 检测 codex 可执行文件
        let install_path = detect::find_in_path("codex");
        let installed = install_path.is_some();

        // 获取版本
        let version = install_path
            .as_ref()
            .and_then(|p| detect::get_version(p, &["--version"]));

        // 检查配置文件
        let codex_home = Self::get_codex_home(home);
        let config_path = codex_home.join("config.toml");
        let config_exists = config_path.exists();

        // 检查 Vortex 配置状态
        let vortex_status = if config_exists {
            match Self::check_vortex_configured(&config_path) {
                Ok(true) => VortexConfigStatus::Configured,
                Ok(false) => VortexConfigStatus::NotConfigured,
                Err(_) => VortexConfigStatus::Failed,
            }
        } else {
            VortexConfigStatus::NotConfigured
        };

        Ok(DetectedAgent {
            kind: AgentKind::Codex,
            installed,
            install_path,
            version,
            config_exists,
            config_path: if config_exists { Some(config_path) } else { None },
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
        let codex_home = Self::get_codex_home(home);
        let config_path = codex_home.join("config.toml");

        let mut files_to_modify = Vec::new();
        let mut warnings = Vec::new();

        if config_path.exists() {
            files_to_modify.push(FileChange {
                path: config_path.clone(),
                operation: FileOperation::Modify,
                summary: "添加 Vortex 提供方配置".to_string(),
            });
        } else {
            files_to_modify.push(FileChange {
                path: config_path,
                operation: FileOperation::Create,
                summary: "创建配置文件并添加 Vortex 提供方".to_string(),
            });
        }

        warnings.push("请确保已设置环境变量 VORTEX_API_KEY".to_string());
        warnings.push("配置完成后需要重启 Codex 才能生效".to_string());

        Ok(ConfigPreview {
            agent: AgentKind::Codex,
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
        let codex_home = Self::get_codex_home(home);
        let config_path = codex_home.join("config.toml");

        // 确保 Codex 目录存在
        std::fs::create_dir_all(&codex_home)
            .map_err(|e| format!("创建 Codex 目录失败: {}", e))?;

        // 创建备份管理器
        let backup_manager = BackupManager::new(backup_dir);

        // 准备要备份的文件
        let mut files_to_backup = Vec::new();
        if config_path.exists() {
            files_to_backup.push((config_path.clone(), None));
        }

        // 创建备份
        let manifest = backup_manager.create_backup(AgentKind::Codex, &files_to_backup)?;

        // 读取或创建现有配置
        let mut doc: DocumentMut = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| format!("读取现有配置失败: {}", e))?;
            content.parse()
                .map_err(|e| format!("解析 TOML 失败: {}", e))?
        } else {
            DocumentMut::new()
        };

        // 确保 [provider] 表存在
        if doc.get("provider").is_none() {
            doc["provider"] = Item::Table(Table::new());
        }

        // 添加 [provider.custom.vortex] 配置
        let provider = doc["provider"].as_table_mut().unwrap();

        if provider.get("custom").is_none() {
            provider["custom"] = Item::Table(Table::new());
        }

        let custom = provider["custom"].as_table_mut().unwrap();

        // 创建 vortex 提供方配置
        let mut vortex_table = Table::new();
        vortex_table["name"] = value("Vortex");
        vortex_table["base_url"] = value(&config.openai_base_url);
        vortex_table["env_key"] = value("VORTEX_API_KEY");

        // 添加模型列表
        let mut models_array = toml_edit::Array::new();
        for model in &config.models {
            models_array.push(model.id.as_str());
        }
        vortex_table["models"] = value(models_array);

        custom["vortex"] = Item::Table(vortex_table);

        // 设置默认模型（如果未设置）
        if let Some(first_model) = config.models.first()
            && provider.get("model").is_none() {
                provider["model"] = value(first_model.id.as_str());
            }

        // 写入配置文件
        let new_content = doc.to_string();

        safe_write::atomic_write_with_backup(&config_path, new_content.as_bytes(), backup_dir)?;

        Ok(ConfigResult {
            agent: AgentKind::Codex,
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
            agent: AgentKind::Codex,
            success: true,
            error: None,
        })
    }
}
