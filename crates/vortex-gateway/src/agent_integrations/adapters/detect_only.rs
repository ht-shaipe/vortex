//! 仅检测适配器
//!
//! 用于暂不支持自动配置的智能体，只提供检测功能。

use std::path::Path;

use crate::agent_integrations::{
    AgentAdapter, AgentKind, ConfigPreview, ConfigResult, DetectedAgent,
    RestoreResult, VortexConfigStatus, VortexGatewayConfig,
};
use crate::agent_integrations::detect;

/// 仅检测适配器
pub struct DetectOnlyAdapter {
    kind: AgentKind,
    reason: &'static str,
}

impl DetectOnlyAdapter {
    /// 创建新的仅检测适配器
    pub fn new(kind: AgentKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }

    /// GitHub Copilot 特殊检测：检查 VS Code / Cursor 扩展目录
    fn detect_copilot(&self, home: &Path) -> DetectedAgent {
        let extension_dirs = [
            home.join(".vscode/extensions"),
            home.join(".cursor/extensions"),
        ];
        let mut found_path = None;
        for dir in &extension_dirs {
            if dir.exists()
                && let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with("github.copilot") {
                            found_path = Some(entry.path());
                            break;
                        }
                    }
                }
            if found_path.is_some() {
                break;
            }
        }
        // 也检查 CLI 可执行文件
        if found_path.is_none() {
            found_path = detect::find_in_path("copilot");
        }

        DetectedAgent {
            kind: self.kind,
            installed: found_path.is_some(),
            install_path: found_path.clone(),
            version: None,
            config_exists: false,
            config_path: None,
            supports_auto_config: false,
            unsupported_reason: Some(self.reason.to_string()),
            vortex_status: VortexConfigStatus::NotConfigured,
        }
    }
}

impl AgentAdapter for DetectOnlyAdapter {
    fn kind(&self) -> AgentKind {
        self.kind
    }

    fn detect(&self, home: &Path) -> Result<DetectedAgent, String> {
        // 根据智能体类型确定可执行文件名
        let exe_name = match self.kind {
            AgentKind::GeminiCli => "gemini",
            AgentKind::CursorAgent => "cursor-agent",
            AgentKind::Windsurf => "windsurf",
            AgentKind::Copilot => "copilot",
            AgentKind::Aider => "aider",
            AgentKind::Zed => "zed",
            AgentKind::Amp => "amp",
            _ => return Err(format!("不支持的智能体类型: {:?}", self.kind)),
        };

        // GitHub Copilot 特殊检测：检查 VS Code 扩展目录
        if self.kind == AgentKind::Copilot {
            return Ok(self.detect_copilot(home));
        }

        // 检测可执行文件
        let install_path = detect::find_in_path(exe_name);
        let installed = install_path.is_some();

        // 获取版本
        let version = install_path
            .as_ref()
            .and_then(|p| detect::get_version(p, &["--version"]));

        // 检查配置文件
        let config_paths = self.kind.config_paths();
        let config_path = config_paths
            .first()
            .map(|p| home.join(p))
            .filter(|p| p.exists());

        Ok(DetectedAgent {
            kind: self.kind,
            installed,
            install_path,
            version,
            config_exists: config_path.is_some(),
            config_path,
            supports_auto_config: false,
            unsupported_reason: Some(self.reason.to_string()),
            vortex_status: VortexConfigStatus::NotConfigured,
        })
    }

    fn preview(
        &self,
        _home: &Path,
        _config: &VortexGatewayConfig,
    ) -> Result<ConfigPreview, String> {
        Err(format!(
            "{} 暂不支持自动配置: {}",
            self.kind.display_name(),
            self.reason
        ))
    }

    fn apply(
        &self,
        _home: &Path,
        _config: &VortexGatewayConfig,
        _backup_dir: &Path,
    ) -> Result<ConfigResult, String> {
        Err(format!(
            "{} 暂不支持自动配置: {}",
            self.kind.display_name(),
            self.reason
        ))
    }

    fn restore(
        &self,
        _home: &Path,
        _backup_id: &str,
        _backup_dir: &Path,
    ) -> Result<RestoreResult, String> {
        Err(format!(
            "{} 暂不支持自动配置: {}",
            self.kind.display_name(),
            self.reason
        ))
    }
}
