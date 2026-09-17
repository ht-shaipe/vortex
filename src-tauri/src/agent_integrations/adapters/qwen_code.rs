//! Qwen Code 适配器
//!
//! 负责检测和配置 Qwen Code 智能体。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent_integrations::{
    AgentAdapter, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    FileChange, FileOperation, RestoreResult, VortexConfigStatus, VortexGatewayConfig,
};
use crate::agent_integrations::backup::BackupManager;
use crate::agent_integrations::detect;
use crate::agent_integrations::safe_write;

/// Qwen Code 适配器
pub struct QwenCodeAdapter;

impl QwenCodeAdapter {
    /// 创建新的适配器
    pub fn new() -> Self {
        Self
    }

    /// 获取 Qwen Code 配置目录
    fn get_qwen_home(home: &Path) -> PathBuf {
        detect::resolve_agent_home("QWEN_HOME", ".qwen").unwrap_or_else(|| home.join(".qwen"))
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

    /// 生成 .env 文件内容
    fn generate_env_content(token: &str, existing: Option<&str>) -> String {
        let mut lines = Vec::new();

        // 保留现有的非 VORTEX_API_KEY 行
        if let Some(content) = existing {
            for line in content.lines() {
                if !line.starts_with("VORTEX_API_KEY=") {
                    lines.push(line.to_string());
                }
            }
        }

        // 添加 VORTEX_API_KEY
        lines.push(format!("VORTEX_API_KEY={}", token));

        lines.join("\n") + "\n"
    }
}

impl AgentAdapter for QwenCodeAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::QwenCode
    }

    fn detect(&self, home: &PathBuf) -> Result<DetectedAgent, String> {
        // 检测 qwen-code 可执行文件
        let install_path = detect::find_in_path("qwen-code");
        let installed = install_path.is_some();

        // 获取版本
        let version = install_path
            .as_ref()
            .and_then(|p| detect::get_version(p, &["--version"]));

        // 检查配置文件
        let qwen_home = Self::get_qwen_home(home);
        let settings_path = qwen_home.join("settings.json");
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
            kind: AgentKind::QwenCode,
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
        let qwen_home = Self::get_qwen_home(home);
        let settings_path = qwen_home.join("settings.json");
        let env_path = qwen_home.join(".env");

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

        // 检查 .env 文件
        if env_path.exists() {
            files_to_modify.push(FileChange {
                path: env_path,
                operation: FileOperation::Modify,
                summary: "添加 VORTEX_API_KEY 环境变量".to_string(),
            });
        } else {
            files_to_modify.push(FileChange {
                path: env_path,
                operation: FileOperation::Create,
                summary: "创建 .env 文件并添加 VORTEX_API_KEY".to_string(),
            });
        }

        warnings.push("配置完成后需要重启 Qwen Code 才能生效".to_string());

        Ok(ConfigPreview {
            agent: AgentKind::QwenCode,
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
        let qwen_home = Self::get_qwen_home(home);
        let settings_path = qwen_home.join("settings.json");
        let env_path = qwen_home.join(".env");

        // 确保 Qwen 目录存在
        std::fs::create_dir_all(&qwen_home)
            .map_err(|e| format!("创建 Qwen 目录失败: {}", e))?;

        // 创建备份管理器
        let backup_manager = BackupManager::new(backup_dir.clone());

        // 准备要备份的文件
        let mut files_to_backup = Vec::new();
        if settings_path.exists() {
            files_to_backup.push((settings_path.clone(), None));
        }
        if env_path.exists() {
            files_to_backup.push((env_path.clone(), None));
        }

        // 创建备份
        let manifest = backup_manager.create_backup(AgentKind::QwenCode, &files_to_backup)?;

        // 读取或创建 settings.json
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
            let existing_idx = providers.iter().position(|p| {
                p.get("name").and_then(|n| n.as_str()) == Some("Vortex")
            });

            if let Some(idx) = existing_idx {
                providers[idx] = vortex_provider;
            } else {
                providers.push(vortex_provider);
            }
        } else {
            settings["providers"] = serde_json::json!([vortex_provider]);
        }

        // 读取或创建 .env 文件
        let existing_env = if env_path.exists() {
            Some(std::fs::read_to_string(&env_path)
                .map_err(|e| format!("读取现有 .env 失败: {}", e))?)
        } else {
            None
        };

        let env_content = Self::generate_env_content(&config.token, existing_env.as_deref());

        // 写入配置文件
        let settings_content = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("序列化 JSON 失败: {}", e))?;

        // 使用事务写入
        let mut tx = safe_write::WriteTransaction::new(backup_dir.clone());
        tx.add_write(settings_path, settings_content.into_bytes());
        tx.add_write(env_path, env_content.into_bytes());

        tx.commit()?;

        Ok(ConfigResult {
            agent: AgentKind::QwenCode,
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
            agent: AgentKind::QwenCode,
            success: true,
            error: None,
        })
    }
}
