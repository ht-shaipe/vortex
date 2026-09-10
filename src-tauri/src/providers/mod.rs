//! 提供商模块入口
//!
//! 定义 AI 提供商的类型结构与内置注册表。

/// 提供商定义类型结构
pub mod types;
/// 内置提供商注册表
pub mod registry;

/// 提供商注册表，重导出供外部使用
pub use registry::ProviderRegistry;
