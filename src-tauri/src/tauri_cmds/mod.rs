//! Tauri 命令模块入口。
//!
//! 该模块汇总了所有暴露给前端调用的 Tauri 命令，按功能领域划分为若干子模块：
//! - `provider_cmds`：提供商相关命令（列表、新增、测试连通性）
//! - `settings_cmds`：应用设置相关命令（读取、更新）
//! - `proxy_cmds`：本地代理服务器的启停命令
//! - `chat_cmds`：对话流式 IPC 命令（Channel 推送 SSE 增量、取消）

/// 提供商相关 Tauri 命令模块。
pub mod provider_cmds;

/// 应用设置相关 Tauri 命令模块。
pub mod settings_cmds;

/// 代理启停相关 Tauri 命令模块。
pub mod proxy_cmds;

/// 系统运行状态相关 Tauri 命令模块。
pub mod status_cmds;

/// 聊天流式对话相关 Tauri 命令模块。
pub mod chat_cmds;

