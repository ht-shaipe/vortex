//! ai-web-provider 库：多平台 AI 网关核心，可被其他项目作为库依赖复用。
//!
//! 模块划分：
//! - [`models`]：命令与数据模型（[`Command`](models::Command)、[`AiReply`](models::AiReply)、
//!   请求/响应结构）。向已启动的平台 worker 发送 [`Command`](models::Command) 即可驱动
//!   对话、登录、调试等全部能力。
//! - [`providers`]：各平台 provider（chatgpt / doubao / deepseek / qianwen / kimi），
//!   [`spawn_platform`](providers::spawn_platform) 按平台名启动独立 worker 线程并返回
//!   命令通道发送端。
//! - [`config`]：环境变量解析（[`CommonCfg`](config::CommonCfg)、
//!   [`ProviderCfg`](config::ProviderCfg)）。
//! - [`server`]：HTTP 服务（AppState + 全部 REST 端点 + OpenAI/Anthropic 兼容路由）。
//!   [`server::configure_routes`] 可嵌入任意 actix-web App；
//!   [`server::serve`] 一键起完整服务。
//! - [`compat`]：OpenAI / Anthropic 协议兼容层端点。
//! - [`auth`]：Bearer Token 认证中间件（[`RequireAuth`](auth::RequireAuth)）。
//! - [`common`]：SSE chunk 协议、表单填充、二维码等工具。
//!
//! 典型嵌入方式（不起 HTTP 服务，直接驱动平台）：
//!
//! ```no_run
//! use std::collections::HashMap;
//! use ai_web_provider::config::CommonCfg;
//! use ai_web_provider::models::Command;
//! use ai_web_provider::providers::spawn_platform;
//!
//! let common = CommonCfg::from_env();
//! let mut workers = HashMap::new();
//! assert!(spawn_platform("deepseek", &common, &mut workers));
//! let tx = workers["deepseek"].clone();
//! // tokio runtime 中：
//! // let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
//! // tx.send(Command::Ask { question: "你好".into(), chunk_tx: None, reply_tx }).await.unwrap();
//! # let _ = tx;
//! ```

pub mod auth;
pub mod cdp_auth;
pub mod common;
pub mod compat;
pub mod config;
pub mod models;
pub mod providers;
pub mod server;
