//! DeepSeek Harness 适配器
//!
//! 负责检测和配置 DeepSeek Harness 智能体。

use std::path::{Path, PathBuf};

use crate::agent_integrations::{
    AgentAdapter, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    FileChange, FileOperation, RestoreResult, VortexConfigStatus, VortexGatewayConfig,
};
use crate::agent_integrations::backup::BackupManager;
use crate::agent_integrations::detect;
use crate::agent_integrations::safe_write;

/// DeepSeek Harness 适配器
pub struct DshAdapter;

impl DshAdapter {
    /// 创建新的适配器
    pub fn new() -> Self {
        Self
    }

    /// 获取 DSH 配置目录
    fn get_dsh_home(home: &Path) -> PathBuf {
        detect::resolve_agent_home("DSH_HOME", ".dsh").unwrap_or_else(|| home.join(".dsh"))
    }

    /// 检查 Vortex 提供方是否已配置
    fn check_vortex_configured(settings_path: &Path) -> Result<bool, String> {
        if !settings_path.exists() {
            return Ok(false);
        }

        let content = std::fs::read_to_string(settings_path)
            .map_err(|e| format!("读取设置文件失败: {}", e))?;

        // 简单检查是否包含 vortex 提供方
        Ok(content.contains("vortex:") && content.contains("baseURL:"))
    }

    /// 生成 Vortex 提供方配置
    fn generate_provider_config(config: &VortexGatewayConfig) -> String {
        let mut yaml = String::new();

        yaml.push_str("llm-pi-ai:\n");
        yaml.push_str("  providers:\n");
        yaml.push_str("    vortex:\n");
        yaml.push_str("      apiKeyEnv: VORTEX_API_KEY\n");
        yaml.push_str("      api: openai-completions\n");
        yaml.push_str(&format!("      baseURL: {}/chat/completions\n", config.openai_base_url));
        yaml.push_str("      models:\n");

        for model in &config.models {
            yaml.push_str(&format!("        - id: {}\n", model.id));
            if let Some(ctx) = model.context_window {
                yaml.push_str(&format!("          contextWindow: {}\n", ctx));
            }
        }

        yaml
    }

    /// 生成凭据配置
    fn generate_credentials_config(token: &str) -> String {
        format!(
            "version: 1\nrefs:\n  VORTEX_API_KEY: {}\n",
            token
        )
    }
}

impl AgentAdapter for DshAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Dsh
    }

    fn detect(&self, home: &PathBuf) -> Result<DetectedAgent, String> {
        // 检测 dsh 可执行文件
        let install_path = detect::find_in_path("dsh");
        let installed = install_path.is_some();

        // 获取版本
        let version = install_path
            .as_ref()
            .and_then(|p| detect::get_version(p, &["--version"]));

        // 检查配置文件
        let dsh_home = Self::get_dsh_home(home);
        let settings_path = dsh_home.join("settings.yaml");
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
            kind: AgentKind::Dsh,
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
        let dsh_home = Self::get_dsh_home(home);
        let settings_path = dsh_home.join("settings.yaml");
        let credentials_path = dsh_home.join(".credentials.yaml");

        let mut files_to_modify = Vec::new();
        let mut warnings = Vec::new();

        // 检查 settings.yaml
        if settings_path.exists() {
            files_to_modify.push(FileChange {
                path: settings_path.clone(),
                operation: FileOperation::Modify,
                summary: "添加 Vortex 提供方配置".to_string(),
            });
        } else {
            files_to_modify.push(FileChange {
                path: settings_path.clone(),
                operation: FileOperation::Create,
                summary: "创建设置文件并添加 Vortex 提供方".to_string(),
            });
        }

        // 检查凭据文件
        if credentials_path.exists() {
            files_to_modify.push(FileChange {
                path: credentials_path.clone(),
                operation: FileOperation::Modify,
                summary: "添加 VORTEX_API_KEY 凭据引用".to_string(),
            });
        } else {
            files_to_modify.push(FileChange {
                path: credentials_path,
                operation: FileOperation::Create,
                summary: "创建凭据文件并添加 VORTEX_API_KEY".to_string(),
            });
        }

        // 检查是否需要重启
        let requires_restart = true; // DSH 需要重启才能加载新配置

        // 警告：如果已有其他默认提供方
        warnings.push("配置完成后需要重启 DeepSeek Harness 才能生效".to_string());

        Ok(ConfigPreview {
            agent: AgentKind::Dsh,
            files_to_modify,
            requires_restart,
            warnings,
        })
    }

    fn apply(
        &self,
        home: &PathBuf,
        config: &VortexGatewayConfig,
        backup_dir: &PathBuf,
    ) -> Result<ConfigResult, String> {
        let dsh_home = Self::get_dsh_home(home);
        let settings_path = dsh_home.join("settings.yaml");
        let credentials_path = dsh_home.join(".credentials.yaml");

        // 确保 DSH 目录存在
        std::fs::create_dir_all(&dsh_home)
            .map_err(|e| format!("创建 DSH 目录失败: {}", e))?;

        // 创建备份管理器
        let backup_manager = BackupManager::new(backup_dir.clone());

        // 准备要备份的文件
        let mut files_to_backup = Vec::new();
        if settings_path.exists() {
            files_to_backup.push((settings_path.clone(), None));
        }
        if credentials_path.exists() {
            files_to_backup.push((credentials_path.clone(), None));
        }

        // 创建备份
        let manifest = backup_manager.create_backup(AgentKind::Dsh, &files_to_backup)?;

        // 生成新的配置内容
        let provider_config = Self::generate_provider_config(config);
        let credentials_config = Self::generate_credentials_config(&config.token);

        // 合并或创建 settings.yaml
        let settings_content = if settings_path.exists() {
            let existing = std::fs::read_to_string(&settings_path)
                .map_err(|e| format!("读取现有设置失败: {}", e))?;
            merge_yaml_config(&existing, &provider_config)?
        } else {
            provider_config
        };

        // 合并或创建凭据文件
        let credentials_content = if credentials_path.exists() {
            let existing = std::fs::read_to_string(&credentials_path)
                .map_err(|e| format!("读取现有凭据失败: {}", e))?;
            merge_credentials(&existing, &config.token)?
        } else {
            credentials_config
        };

        // 使用事务写入
        let mut tx = safe_write::WriteTransaction::new(backup_dir.clone());
        tx.add_write(settings_path, settings_content.into_bytes());
        tx.add_write(credentials_path, credentials_content.into_bytes());

        tx.commit()?;

        Ok(ConfigResult {
            agent: AgentKind::Dsh,
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

        // 验证备份完整性
        if !backup_manager.verify_backup(backup_id)? {
            return Err("备份文件不完整或已损坏".to_string());
        }

        // 恢复每个文件
        for file in &manifest.files {
            if file.existed {
                // 恢复原文件
                std::fs::copy(&file.backup_path, &file.original_path)
                    .map_err(|e| format!("恢复文件失败 {}: {}", file.original_path.display(), e))?;
            } else {
                // 删除新创建的文件
                if file.original_path.exists() {
                    std::fs::remove_file(&file.original_path)
                        .map_err(|e| format!("删除文件失败 {}: {}", file.original_path.display(), e))?;
                }
            }
        }

        Ok(RestoreResult {
            agent: AgentKind::Dsh,
            success: true,
            error: None,
        })
    }
}

/// 合并 YAML 配置（简化版本，实际应该使用 yaml-edit 库）
fn merge_yaml_config(existing: &str, new_provider: &str) -> Result<String, String> {
    // 如果现有配置已包含 llm-pi-ai.providers，尝试合并
    if existing.contains("llm-pi-ai:") && existing.contains("providers:") {
        // 简化处理：在 providers 部分末尾添加新的提供方
        // 实际应该使用 yaml-edit 进行精确合并
        let mut result = existing.to_string();

        // 查找 providers 部分的结尾
        if let Some(pos) = result.find("providers:") {
            // 在 providers: 后面插入新配置
            let insert_pos = pos + "providers:".len();
            let new_part = &new_provider[new_provider.find("providers:").unwrap() + "providers:".len()..];
            result.insert_str(insert_pos, new_part);
            return Ok(result);
        }
    }

    // 如果没有找到现有配置，直接返回新配置
    Ok(new_provider.to_string())
}

/// 合并凭据配置
fn merge_credentials(existing: &str, token: &str) -> Result<String, String> {
    // 如果已有 version: 1 和 refs，添加新的引用
    if existing.contains("version: 1") && existing.contains("refs:") {
        let mut result = existing.to_string();

        // 在 refs: 部分添加新的引用
        if let Some(pos) = result.find("refs:") {
            let insert_pos = pos + "refs:".len();
            result.insert_str(insert_pos, &format!("\n  VORTEX_API_KEY: {}", token));
            return Ok(result);
        }
    }

    // 否则创建新的凭据文件
    Ok(format!("version: 1\nrefs:\n  VORTEX_API_KEY: {}\n", token))
}
