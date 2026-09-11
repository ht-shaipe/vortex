//! 数据模型定义模块。
//!
//! 定义所有与数据库交互及前后端序列化相关的结构体，包括：
//! 提供商连接（`ProviderConnection`）、API 密钥（`ApiKey`）、用量记录（`UsageEntry`）、
//! 用量统计（`UsageStats`）、代理请求（`ProxyRequest`）、免费 Token 站点（`FreeTokenSite`）等。

use serde::{Deserialize, Serialize};

/// 连接下挂载的模型条目：id 为实际发送给上游 API 的模型名，
/// name 为用户自定义的展示名称（可选；留空时界面回退到 id 显示）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderModel {
    /// 实际发送给上游 API 的模型标识
    pub id: String,
    /// 用户自定义的展示名称（可选；留空时界面回退到 id 显示）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// 提供商连接实体，对应 `provider_connections` 表的完整记录。
///
/// 描述一个上游 AI 服务连接的全部配置与运行时状态，包括认证信息、
/// 优先级、健康状态、限流退避、代理设置、挂载模型等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConnection {
    /// 连接唯一标识（UUID）
    pub id: String,
    /// 提供商标识（如 `openai`、`anthropic`）
    pub provider: String,
    /// 认证类型（如 `apikey`、`oauth`）
    pub auth_type: String,
    /// 连接名称（用户可读）
    pub name: String,
    /// 关联邮箱（OAuth 场景使用）
    pub email: Option<String>,
    /// 优先级，数值越大越优先
    pub priority: i32,
    /// 是否启用
    pub is_active: bool,
    /// OAuth access token（加密存储）
    pub access_token: Option<String>,
    /// OAuth refresh token（加密存储）
    pub refresh_token: Option<String>,
    /// OAuth token 过期时间
    pub expires_at: Option<String>,
    /// API Key（加密存储）
    pub api_key: Option<String>,
    /// OAuth id token
    pub id_token: Option<String>,
    /// 项目 ID（如 GCP project）
    pub project_id: Option<String>,
    /// 测试状态（`unknown` / `ok` / `fail`）
    pub test_status: String,
    /// 最近错误码
    pub error_code: Option<String>,
    /// 最近错误信息
    pub last_error: Option<String>,
    /// 最近错误发生时间
    pub last_error_at: Option<String>,
    /// 退避级别（限流后递增，成功后归零）
    pub backoff_level: i32,
    /// 限流恢复时间（达到此时间后可重试）
    pub rate_limited_until: Option<String>,
    /// 健康检查间隔（秒）
    pub health_check_interval: i32,
    /// 连续使用计数
    pub consecutive_use_count: i32,
    /// 是否启用限流保护
    pub rate_limit_protection: bool,
    /// 分组名称（同组连接可做负载均衡）
    pub group_name: Option<String>,
    /// 最大并发数限制
    pub max_concurrent: Option<i32>,
    /// 是否启用代理
    pub proxy_enabled: bool,
    /// 列表展示名称（自定义提供方场景使用）
    pub display_name: Option<String>,
    /// 默认模型（路由回退时使用）
    pub default_model: Option<String>,
    /// 该连接下挂载的模型列表（含自定义展示名）。为空时回退到 default_model。
    pub models: Option<Vec<ProviderModel>>,
    /// OAuth token 类型
    pub token_type: Option<String>,
    /// OAuth scope
    pub scope: Option<String>,
    /// 最近使用时间
    pub last_used_at: Option<String>,
    /// 最近健康检查时间
    pub last_health_check_at: Option<String>,
    /// 最近测试时间
    pub last_tested: Option<String>,
    /// 提供方特定数据（JSON，含 baseUrl、apiProtocol、customId、models 等）
    pub provider_specific_data: serde_json::Value,
    /// 创建时间（RFC3339）
    pub created_at: String,
    /// 更新时间（RFC3339）
    pub updated_at: String,
    /// 最近一次连接测试的响应延迟（毫秒）。列表首列展示，未测试时为 None。
    pub last_latency_ms: Option<i64>,
}

/// 创建提供商连接的请求体。
///
/// 前端提交的连接创建参数，字段大多可选，由后端填充默认值。
/// 序列化/反序列化使用 camelCase 与前端对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderRequest {
    /// 提供商标识
    pub provider: String,
    /// 认证类型（默认 `apikey`）
    pub auth_type: Option<String>,
    /// 连接名称
    pub name: String,
    /// 关联邮箱
    pub email: Option<String>,
    /// API Key
    pub api_key: Option<String>,
    /// OAuth access token
    pub access_token: Option<String>,
    /// OAuth refresh token
    pub refresh_token: Option<String>,
    /// 项目 ID
    pub project_id: Option<String>,
    /// 基础 URL（自定义提供方使用）
    pub base_url: Option<String>,
    /// 优先级（默认 0）
    pub priority: Option<i32>,
    /// 分组名称
    pub group_name: Option<String>,
    /// 最大并发数
    pub max_concurrent: Option<i32>,
    /// 默认模型
    pub default_model: Option<String>,
    /// 该连接下挂载的模型列表（含自定义展示名）。为空时回退到 default_model。
    pub models: Option<Vec<ProviderModel>>,
    /// 列表里展示给用户的名字。自定义提供方场景必填，用于在已有内置 provider 中区分。
    pub display_name: Option<String>,
    /// API 协议（openai / anthropic / gemini / cloudflare / cohere）。仅自定义提供方使用。
    pub api_protocol: Option<String>,
    /// 用户填的 Provider ID（如 `acme-gateway`）。仅自定义提供方使用，写入 `provider_specific_data.customId`。
    pub custom_provider_id: Option<String>,
    /// 提供方特定数据（JSON）
    pub provider_specific_data: Option<serde_json::Value>,
}

/// API 密钥实体，对应 `api_keys` 表的完整记录。
///
/// 描述一个用户面向的 API 密钥及其权限、限流、使用限制等配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// 密钥唯一标识（UUID）
    pub id: String,
    /// 密钥名称（用户可读）
    pub name: String,
    /// 密钥值（格式 `vx-{uuid}`）
    pub key: String,
    /// 绑定的机器 ID
    pub machine_id: Option<String>,
    /// 允许的模型列表（JSON 数组）
    pub allowed_models: serde_json::Value,
    /// 允许的连接列表（JSON 数组）
    pub allowed_connections: serde_json::Value,
    /// 允许的端点列表（JSON 数组）
    pub allowed_endpoints: serde_json::Value,
    /// 是否禁用日志记录
    pub no_log: bool,
    /// 是否启用自动路由解析
    pub auto_resolve: bool,
    /// 是否启用
    pub is_active: bool,
    /// 是否被封禁
    pub is_banned: bool,
    /// 速率限制配置（JSON）
    pub rate_limits: serde_json::Value,
    /// 用量限制配置（JSON）
    pub usage_limits: serde_json::Value,
    /// 创建时间（RFC3339）
    pub created_at: String,
}

/// 创建 API 密钥的请求体。
///
/// 前端提交的密钥创建参数，限流与用量限制可选。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    /// 密钥名称
    pub name: String,
    /// 允许的模型列表（默认空数组）
    pub allowed_models: Option<serde_json::Value>,
    /// 速率限制配置（默认空数组）
    pub rate_limits: Option<serde_json::Value>,
    /// 用量限制配置（默认空对象）
    pub usage_limits: Option<serde_json::Value>,
}

/// 单条用量记录实体，对应 `usage_history` 表的一条记录。
///
/// 记录一次 API 代理请求的完整信息：提供方、模型、Token 用量、延迟、费用等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    /// 记录自增 ID
    pub id: i64,
    /// 提供方名称
    pub provider: Option<String>,
    /// 模型名称
    pub model: Option<String>,
    /// 连接 ID
    pub connection_id: Option<String>,
    /// API 密钥 ID
    pub api_key_id: Option<String>,
    /// API 密钥名称
    pub api_key_name: Option<String>,
    /// 输入 Token 数
    pub tokens_input: i64,
    /// 输出 Token 数
    pub tokens_output: i64,
    /// 缓存读取 Token 数
    pub tokens_cache_read: i64,
    /// 缓存创建 Token 数
    pub tokens_cache_creation: i64,
    /// 推理 Token 数
    pub tokens_reasoning: i64,
    /// 服务层级
    pub service_tier: String,
    /// 请求状态
    pub status: String,
    /// 是否成功
    pub success: bool,
    /// 错误码
    pub error_code: Option<String>,
    /// 总延迟（毫秒）
    pub latency_ms: Option<i64>,
    /// 首字延迟（毫秒）
    pub ttft_ms: Option<i64>,
    /// 费用
    pub cost: f64,
    /// 时间戳（RFC3339）
    pub timestamp: String,
}

/// 用量统计聚合结果。
///
/// 对 `usage_history` 表按时间范围聚合后的统计概览，包含总量指标和按提供方/模型/日期的分维度统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    /// 总请求数
    pub total_requests: i64,
    /// 总输入 Token 数
    pub total_tokens_input: i64,
    /// 总输出 Token 数
    pub total_tokens_output: i64,
    /// 总费用
    pub total_cost: f64,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: f64,
    /// 成功率（0.0 ~ 1.0）
    pub success_rate: f64,
    /// 按提供方分组的请求数（JSON 对象）
    pub by_provider: serde_json::Value,
    /// 按模型分组的请求数（JSON 对象）
    pub by_model: serde_json::Value,
    /// 按日期分组的统计（JSON 对象，最近 30 天）
    pub by_day: serde_json::Value,
}

/// 代理请求体，描述一次转发给上游 API 的请求参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    /// 目标模型名
    pub model: String,
    /// 消息列表（JSON）
    pub messages: serde_json::Value,
    /// 是否流式
    pub stream: bool,
    /// 温度参数
    pub temperature: Option<f64>,
    /// 最大输出 Token 数
    pub max_tokens: Option<i64>,
    /// top_p 参数
    pub top_p: Option<f64>,
    /// 调用方 API Key
    pub api_key: Option<String>,
    /// 源格式（如 `openai`、`anthropic`）
    pub source_format: String,
    /// 额外参数（JSON）
    pub extra: serde_json::Value,
}

/// 免费 Token 站点目录条目。
/// 请求与响应统一使用 camelCase，避免前后端命名不一致导致的字段丢失。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeTokenSite {
    /// 站点唯一标识（UUID）
    pub id: String,
    /// 站点名称
    pub name: String,
    /// 主页 URL
    pub home_url: String,
    /// 申请 URL
    pub apply_url: String,
    /// 是否支持 API 调用
    pub api_supported: bool,
    /// API 基础 URL
    pub api_base: Option<String>,
    /// API 格式（如 `openai`）
    pub api_format: Option<String>,
    /// 免费额度描述
    pub free_quota: String,
    /// 地区
    pub region: String,
    /// 是否需要信用卡
    pub requires_card: bool,
    /// 是否需要验证
    pub requires_verify: bool,
    /// 标签（JSON 数组）
    pub tags: serde_json::Value,
    /// 备注
    pub note: Option<String>,
    /// 关联的提供方 ID
    pub provider_id: Option<String>,
    /// 来源（`builtin` 内置 / `user` 用户提交）
    pub source: String,
    /// 提交者
    pub submitter: Option<String>,
    /// 排序权重（越小越靠前）
    pub sort_order: i32,
    /// 创建时间（RFC3339）
    pub created_at: String,
    /// 更新时间（RFC3339）
    pub updated_at: String,
}

/// 用户提交的站点推荐。仅 `name` 为必填，其余字段留空时由后端补默认值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFreeTokenSiteRequest {
    /// 站点名称（必填）
    pub name: String,
    /// 主页 URL
    #[serde(default)]
    pub home_url: Option<String>,
    /// 申请 URL
    #[serde(default)]
    pub apply_url: Option<String>,
    /// 是否支持 API 调用
    #[serde(default)]
    pub api_supported: Option<bool>,
    /// API 基础 URL
    #[serde(default)]
    pub api_base: Option<String>,
    /// API 格式
    #[serde(default)]
    pub api_format: Option<String>,
    /// 免费额度描述
    #[serde(default)]
    pub free_quota: Option<String>,
    /// 地区
    #[serde(default)]
    pub region: Option<String>,
    /// 是否需要信用卡
    #[serde(default)]
    pub requires_card: Option<bool>,
    /// 是否需要验证
    #[serde(default)]
    pub requires_verify: Option<bool>,
    /// 标签（JSON 数组）
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
    /// 备注
    #[serde(default)]
    pub note: Option<String>,
    /// 关联的提供方 ID
    #[serde(default)]
    pub provider_id: Option<String>,
    /// 提交者
    #[serde(default)]
    pub submitter: Option<String>,
}
