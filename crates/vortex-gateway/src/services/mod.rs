//! 业务逻辑服务层。
//!
//! 本模块将对前端暴露的业务逻辑从 Tauri 命令和 HTTP 端点中提取出来，
//! 形成独立的服务函数，供 Tauri IPC 和 actix-web 两种调用方式复用。
//!
//! - `provider_service`：提供商列表、新增、测试
//! - `settings_service`：应用设置读取与更新
//! - `system_service`：系统运行状态快照
//! - `proxy_service`：本地代理服务器启停
//! - `chat_service`：对话流取消管理
//! - `agent_service`：智能体集成检测、配置预览/应用/恢复、备份列表

pub mod provider_service;
pub mod settings_service;
pub mod system_service;
pub mod proxy_service;
pub mod chat_service;
pub mod agent_service;
