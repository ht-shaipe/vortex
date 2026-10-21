//! Web（浏览器自动化）提供商桥接抽象。
//!
//! vortex-router 不直接依赖 obscura/V8 技术栈（ai-web-provider），
//! 通过 [`WebChatBridge`] trait 与网页版平台 worker 解耦：
//! 上层 crate（vortex-gateway）持有 ai-web-provider 的平台 worker
//! 并实现本 trait，经 [`EngineContext`](crate::context::EngineContext)
//! 注入到代理引擎的执行链，由 [`WebExecutor`](crate::proxy::executor::WebExecutor)
//! 消费。
//!
//! 网页版会话语义（与 API 提供商的差异）：
//! - 有状态：浏览器聊天页自带多轮上下文，每次请求只下发
//!   system 前缀 + 最后一条 user 消息，不回放历史
//!   （避免往浏览器会话里重复追加整段对话）。
//! - 单 worker 串行：同一平台所有请求排队执行（避免互相串台）。

use std::future::Future;
use std::pin::Pin;

/// web 平台回复（对应 ai-web-provider 的 AiReply 精简版，
/// 避免 router 依赖 V8 技术栈的类型定义）
#[derive(Debug, Clone, Default)]
pub struct WebReply {
    /// 深度思考过程（未触发深度思考时为空）
    pub thinking: String,
    /// 最终答案文本
    pub reply: String,
}

/// 流式增量类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebChunkKind {
    /// 思考过程增量 → OpenAI `delta.reasoning_content`
    Thinking,
    /// 正文增量 → OpenAI `delta.content`
    Text,
    /// 过程提示（如自动拉起浏览器、收集登录凭证）→ OpenAI
    /// `delta.vortex_status`，不计入正文与 token 统计
    Status,
}

/// 单个流式增量
#[derive(Debug, Clone)]
pub struct WebChunk {
    pub kind: WebChunkKind,
    pub text: String,
}

/// 流式 ask 返回的句柄。
/// chunks 流结束后 final_reply 才 resolve（携带完整回复或错误）。
pub struct WebStreamHandle {
    /// 增量流（发送端关闭即增量结束）
    pub chunks: Pin<Box<dyn Stream<Item = WebChunk> + Send>>,
    /// 最终完整回复
    pub final_reply: Pin<Box<dyn Future<Output = std::result::Result<WebReply, String>> + Send>>,
}

use futures::Stream;

/// 网页版平台聊天桥接。
///
/// 由 vortex-gateway 侧实现（包装 ai-web-provider 的平台 worker 命令通道）。
#[async_trait::async_trait]
pub trait WebChatBridge: Send + Sync {
    /// 平台 worker 是否已启动（管理页状态展示；聊天路径由 bridge 按需拉起）
    fn available(&self, platform: &str) -> bool;

    /// 非流式 ask：阻塞至回复完整
    async fn ask(&self, platform: &str, question: String) -> std::result::Result<WebReply, String>;

    /// 流式 ask：返回增量流 + 最终结果
    async fn ask_stream(
        &self,
        platform: &str,
        question: String,
    ) -> std::result::Result<WebStreamHandle, String>;
}
