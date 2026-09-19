//! 智能体集成模块
//!
//! 负责检测本机已安装的 AI 编程智能体，并自动将 Vortex 网关配置写入其配置文件，
//! 使用户无需手动配置即可在各大智能体中使用 Vortex 路由的模型。
//!
//! 设计原则：
//! - 只修改受管的 Vortex 节点，保留其他配置不变
//! - 写入前备份，支持恢复
//! - 不读取或泄露用户凭据
//! - 幂等操作，重复配置不产生副作用

pub mod adapters;
pub mod backup;
pub mod detect;
pub mod safe_write;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 智能体枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// DeepSeek Harness
    Dsh,
    /// Claude Code
    ClaudeCode,
    /// OpenCode
    #[serde(rename = "opencode")]
    OpenCode,
    /// OpenAI Codex
    Codex,
    /// Qwen Code
    QwenCode,
    /// Gemini CLI (仅检测)
    GeminiCli,
    /// Cursor Agent (仅检测)
    CursorAgent,
    /// Windsurf / Codeium (仅检测)
    Windsurf,
    /// GitHub Copilot (仅检测)
    Copilot,
    /// Aider (仅检测)
    Aider,
    /// Zed Editor (仅检测)
    Zed,
    /// Amp (仅检测)
    Amp,
}

impl AgentKind {
    /// 所有已知智能体
    pub const ALL: &'static [AgentKind] = &[
        AgentKind::Dsh,
        AgentKind::ClaudeCode,
        AgentKind::OpenCode,
        AgentKind::Codex,
        AgentKind::QwenCode,
        AgentKind::GeminiCli,
        AgentKind::CursorAgent,
        AgentKind::Windsurf,
        AgentKind::Copilot,
        AgentKind::Aider,
        AgentKind::Zed,
        AgentKind::Amp,
    ];

    /// 智能体显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentKind::Dsh => "DeepSeek Harness",
            AgentKind::ClaudeCode => "Claude Code",
            AgentKind::OpenCode => "OpenCode",
            AgentKind::Codex => "Codex",
            AgentKind::QwenCode => "Qwen Code",
            AgentKind::GeminiCli => "Gemini CLI",
            AgentKind::CursorAgent => "Cursor Agent",
            AgentKind::Windsurf => "Windsurf",
            AgentKind::Copilot => "GitHub Copilot",
            AgentKind::Aider => "Aider",
            AgentKind::Zed => "Zed",
            AgentKind::Amp => "Amp",
        }
    }

    /// 是否支持自动配置
    ///
    /// 仅在测试断言中使用（非 test 构建下标记 dead_code 抑制告警）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn supports_auto_config(&self) -> bool {
        matches!(
            self,
            AgentKind::Dsh
                | AgentKind::ClaudeCode
                | AgentKind::OpenCode
                | AgentKind::Codex
                | AgentKind::QwenCode
        )
    }

    /// 配置文件路径（相对于用户主目录）
    pub fn config_paths(&self) -> Vec<&'static str> {
        match self {
            AgentKind::Dsh => vec![".dsh/settings.yaml", ".dsh/.credentials.yaml"],
            AgentKind::ClaudeCode => vec![".claude/settings.json"],
            AgentKind::OpenCode => vec![".config/opencode/opencode.json"],
            AgentKind::Codex => vec![".codex/config.toml"],
            AgentKind::QwenCode => vec![".qwen/settings.json", ".qwen/.env"],
            AgentKind::GeminiCli => vec![".gemini/settings.json"],
            AgentKind::CursorAgent => vec![],
            AgentKind::Windsurf => vec![".codeium/windsurf/config.json"],
            AgentKind::Copilot => vec![],
            AgentKind::Aider => vec![".aider.conf.yml"],
            AgentKind::Zed => vec![".config/zed/settings.json"],
            AgentKind::Amp => vec![".amp/settings.json"],
        }
    }
}

/// 检测到的智能体信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    /// 智能体类型
    pub kind: AgentKind,
    /// 是否已安装（可执行文件存在）
    pub installed: bool,
    /// 安装路径（如果检测到）
    pub install_path: Option<PathBuf>,
    /// 版本号（如果检测到）
    pub version: Option<String>,
    /// 配置文件是否存在
    pub config_exists: bool,
    /// 配置文件路径
    pub config_path: Option<PathBuf>,
    /// 是否支持自动配置
    pub supports_auto_config: bool,
    /// 不支持自动配置的原因
    pub unsupported_reason: Option<String>,
    /// Vortex 配置状态
    pub vortex_status: VortexConfigStatus,
}

/// Vortex 配置状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VortexConfigStatus {
    /// 未配置
    NotConfigured,
    /// 已配置且最新
    Configured,
    /// 已配置但需要更新（端口/令牌/模型变化）
    NeedsUpdate,
    /// 配置冲突（存在同名非受管节点）
    Conflict,
    /// 配置失败
    Failed,
}

/// 配置预览（脱敏）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPreview {
    /// 智能体类型
    pub agent: AgentKind,
    /// 将要修改的文件列表
    pub files_to_modify: Vec<FileChange>,
    /// 是否需要重启智能体
    pub requires_restart: bool,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// 文件变更预览
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// 文件路径
    pub path: PathBuf,
    /// 操作类型
    pub operation: FileOperation,
    /// 变更摘要（脱敏）
    pub summary: String,
}

/// 文件操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    /// 创建新文件
    Create,
    /// 修改现有文件
    Modify,
    /// 删除文件（恢复时）
    Delete,
}

/// 配置应用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResult {
    /// 智能体类型
    pub agent: AgentKind,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 备份 ID（用于恢复）
    pub backup_id: Option<String>,
    /// 是否需要重启
    pub requires_restart: bool,
}

/// 恢复结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    /// 智能体类型
    pub agent: AgentKind,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
}

/// Vortex 网关配置（用于写入智能体配置）
#[derive(Debug, Clone)]
pub struct VortexGatewayConfig {
    /// 网关端口
    ///
    /// 预留：部分智能体需要独立的主机+端口拼接（非完整 base URL）。
    #[allow(dead_code)]
    pub port: u16,
    /// 访问令牌
    pub token: String,
    /// 可用模型列表
    pub models: Vec<ModelInfo>,
    /// OpenAI 兼容基础 URL
    pub openai_base_url: String,
    /// Anthropic 兼容基础 URL
    ///
    /// 预留：走 Anthropic 原生协议的智能体（如 Claude Code）后续切换用。
    #[allow(dead_code)]
    pub anthropic_base_url: String,
}

/// 模型信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 模型 ID
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 提供方
    pub provider: String,
    /// 上下文窗口
    pub context_window: Option<u64>,
}

impl VortexGatewayConfig {
    /// 创建网关配置
    pub fn new(port: u16, token: String, models: Vec<ModelInfo>) -> Self {
        let openai_base_url = format!("http://127.0.0.1:{}/v1", port);
        let anthropic_base_url = format!("http://127.0.0.1:{}", port);
        Self {
            port,
            token,
            models,
            openai_base_url,
            anthropic_base_url,
        }
    }
}

/// 智能体适配器特征
pub trait AgentAdapter: Send + Sync {
    /// 获取智能体类型
    ///
    /// 预留：供后续泛型遍历/日志场景使用；当前注册表以 key 定位适配器。
    #[allow(dead_code)]
    fn kind(&self) -> AgentKind;

    /// 检测智能体是否已安装
    fn detect(&self, home: &Path) -> Result<DetectedAgent, String>;

    /// 生成配置预览
    fn preview(
        &self,
        home: &Path,
        config: &VortexGatewayConfig,
    ) -> Result<ConfigPreview, String>;

    /// 应用配置
    fn apply(
        &self,
        home: &Path,
        config: &VortexGatewayConfig,
        backup_dir: &Path,
    ) -> Result<ConfigResult, String>;

    /// 恢复配置
    fn restore(
        &self,
        home: &Path,
        backup_id: &str,
        backup_dir: &Path,
    ) -> Result<RestoreResult, String>;
}

/// 适配器注册表
pub struct AdapterRegistry {
    adapters: HashMap<AgentKind, Box<dyn AgentAdapter>>,
}

impl AdapterRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        let mut adapters: HashMap<AgentKind, Box<dyn AgentAdapter>> = HashMap::new();

        // 注册所有适配器
        adapters.insert(AgentKind::Dsh, Box::new(adapters::dsh::DshAdapter::new()));
        adapters.insert(
            AgentKind::ClaudeCode,
            Box::new(adapters::claude_code::ClaudeCodeAdapter::new()),
        );
        adapters.insert(
            AgentKind::OpenCode,
            Box::new(adapters::opencode::OpenCodeAdapter::new()),
        );
        adapters.insert(AgentKind::Codex, Box::new(adapters::codex::CodexAdapter::new()));
        adapters.insert(
            AgentKind::QwenCode,
            Box::new(adapters::qwen_code::QwenCodeAdapter::new()),
        );
        adapters.insert(
            AgentKind::GeminiCli,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::GeminiCli,
                "Gemini CLI 暂不支持自动配置，请手动设置 base_url",
            )),
        );
        adapters.insert(
            AgentKind::CursorAgent,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::CursorAgent,
                "Cursor 需要通过 IDE 设置界面配置",
            )),
        );
        adapters.insert(
            AgentKind::Windsurf,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::Windsurf,
                "Windsurf 暂不支持自动配置，请手动设置 base_url",
            )),
        );
        adapters.insert(
            AgentKind::Copilot,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::Copilot,
                "GitHub Copilot 通过 VS Code 设置界面配置",
            )),
        );
        adapters.insert(
            AgentKind::Aider,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::Aider,
                "Aider 暂不支持自动配置，请在 .aider.conf.yml 中手动设置",
            )),
        );
        adapters.insert(
            AgentKind::Zed,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::Zed,
                "Zed 暂不支持自动配置，请在 settings.json 中手动设置",
            )),
        );
        adapters.insert(
            AgentKind::Amp,
            Box::new(adapters::detect_only::DetectOnlyAdapter::new(
                AgentKind::Amp,
                "Amp 暂不支持自动配置，请手动设置 base_url",
            )),
        );

        Self { adapters }
    }

    /// 获取适配器
    pub fn get(&self, kind: AgentKind) -> Option<&dyn AgentAdapter> {
        self.adapters.get(&kind).map(|a| a.as_ref())
    }

    /// 检测所有智能体
    pub fn detect_all(&self, home: &Path) -> Vec<DetectedAgent> {
        AgentKind::ALL
            .iter()
            .filter_map(|kind| {
                self.get(*kind)
                    .and_then(|adapter| adapter.detect(home).ok())
            })
            .collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
