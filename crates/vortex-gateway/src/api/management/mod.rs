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
/// 模型别名（虚拟模型映射）管理端点
pub mod model_aliases;
/// 速率限制管理端点
pub mod rate_limits;
/// 批量密钥导入/导出端点
pub mod bulk_keys;
/// 路由配置文件管理端点
pub mod routing_profiles;
/// ToS 合规审查端点
pub mod tos_review;
/// 加密数据库备份端点
pub mod backup;
/// 模型目录管理端点
pub mod model_catalog;
/// 延迟统计端点
pub mod latency_stats;
/// Playground 测试端点
pub mod playground;
/// 成本分析端点
pub mod cost_analysis;
/// 智能推荐端点
pub mod recommendations;
/// 内容寻址回忆端点
pub mod compressed_content;
/// API Key 权限管理端点
pub mod key_permissions;
/// 系统状态与代理控制端点
pub mod system;
/// 智能体集成端点
pub mod agents;
/// 对话控制端点
pub mod chat;
