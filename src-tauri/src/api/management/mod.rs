//! 管理 API 端点模块
//!
//! 该模块聚合了所有面向本地前端 UI 的管理端点，涵盖：
//! - [`providers`]：提供商连接的增删改查与连接测试、模型预览。
//! - [`keys`]：本地 API 密钥管理。
//! - [`usage`]：用量记录查询与统计。
//! - [`settings`]：全局设置读写。
//! - [`health`]：健康检查。
//! - [`free_tokens`]：免费 Token 站点管理。

/// 提供商连接管理端点
pub mod providers;
/// API 密钥管理端点
pub mod keys;
/// 用量记录与统计端点
pub mod usage;
/// 设置管理端点
pub mod settings;
/// 健康检查端点
pub mod health;
/// 免费 Token 站点管理端点
pub mod free_tokens;
