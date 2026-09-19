//! Vortex 数据存储层与基础设施。
//!
//! 包含统一错误类型 [`error::AppError`]、应用配置 [`config::AppConfig`]、
//! SQLite 数据库连接池与所有业务表的 CRUD 操作。

pub mod error;
pub mod config;
pub mod db;
