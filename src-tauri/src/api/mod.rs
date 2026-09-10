//! API 模块入口
//!
//! 该模块是 Vortex HTTP 服务的顶层 API 聚合点，按功能域划分为两个子模块：
//! - [`v1`]：OpenAI / Anthropic 兼容的代理端点，面向客户端 SDK 直连。
//! - [`management`]：本地管理端点，供前端 UI 进行配置与运维操作。

/// OpenAI / Anthropic 兼容的 v1 代理端点
pub mod v1;
/// 本地管理端点（提供商、密钥、用量、设置等）
pub mod management;
