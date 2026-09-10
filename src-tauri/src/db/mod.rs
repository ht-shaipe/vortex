//! 数据库模块入口。
//!
//! 汇总所有数据库相关子模块：连接池初始化、数据模型定义、迁移管理、
//! 加密工具、以及各业务表（提供商连接、API 密钥、用量记录、设置、免费 Token）的 CRUD 操作。

/// SQLite 连接池初始化与连接获取
pub mod core;
/// 简单迁移系统，通过 `_vortex_migrations` 表跟踪已应用的迁移
pub mod migration;
/// 所有数据模型结构体定义
pub mod models;
/// 提供商连接 CRUD，敏感字段加密/解密
pub mod providers;
/// API 密钥 CRUD，密钥格式 `vx-{uuid}`
pub mod api_keys;
/// 用量记录与统计查询
pub mod usage;
/// 设置存储（`key_value` 表）
pub mod settings;
/// 免费 Token 站点管理
pub mod free_tokens;
/// AES-256-GCM 加密/解密、密钥派生与密钥生成
pub mod encryption;
