//! Vortex 路由引擎核心。
//!
//! 包含代理引擎（proxy）、提供商注册表（providers）、路由策略（routing）、
//! 格式转换器（translator）和远程代理（remote_proxy）。
//! 定义 [`context::EngineContext`] trait，解耦引擎与上层应用状态。

pub mod context;
pub mod proxy;
pub mod providers;
pub mod routing;
pub mod translator;
pub mod remote_proxy;
