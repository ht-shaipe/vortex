//! 代理模块入口
//!
//! 汇集代理引擎的各子模块，负责将入站请求路由到上游 AI 提供商并归一化响应格式。

/// 核心代理引擎，处理请求路由、重试与用量记录
pub mod engine;
/// SSE 流式响应处理，将上游流归一化为 OpenAI chunk 格式
pub mod sse;
/// 重试策略，支持指数退避
pub mod retry;
/// 上游执行器，按 API 格式构建并发送请求
pub mod executor;
