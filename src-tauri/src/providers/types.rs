//! 提供商定义类型
//!
//! 定义描述一个 AI 提供商所需的全部字段，包括 API 端点、认证方式、
//! 支持的服务类型等。该结构体同时用于内置注册表和运行时连接配置。

use serde::{Deserialize, Serialize};

/// 提供商定义结构体
///
/// 描述一个 AI 服务提供商的静态配置信息，包括 API 端点路径、
/// 认证方式、支持的模型格式等。每个内置提供商在注册表中对应一个实例。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDef {
    /// 提供商唯一标识符（如 "openai"、"anthropic"）
    pub id: String,
    /// 提供商短别名（如 "oa"、"an"），用于模型名前缀匹配
    pub alias: String,
    /// 提供商显示名称
    pub name: String,
    /// 图标标识
    pub icon: String,
    /// 主题颜色（十六进制色值）
    pub color: String,
    /// 支持的服务类型列表（如 "llm"、"embedding"、"image"）
    pub service_kinds: Vec<String>,
    /// 是否无需认证
    pub no_auth: bool,
    /// 是否提供免费额度
    pub has_free: bool,
    /// 免费额度说明
    pub free_note: Option<String>,
    /// 认证提示信息（如 API key 获取地址）
    pub auth_hint: Option<String>,
    /// API 基础 URL
    pub base_url: String,
    /// 聊天补全接口路径
    pub chat_path: String,
    /// 模型列表接口路径
    pub models_path: String,
    /// API 响应格式（"openai"、"anthropic"、"gemini" 等）
    pub api_format: String,
    /// 认证类型（"apikey"、"noauth" 等）
    pub auth_type: String,
}
