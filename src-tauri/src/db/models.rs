use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConnection {
    pub id: String,
    pub provider: String,
    pub auth_type: String,
    pub name: String,
    pub email: Option<String>,
    pub priority: i32,
    pub is_active: bool,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<String>,
    pub api_key: Option<String>,
    pub id_token: Option<String>,
    pub project_id: Option<String>,
    pub test_status: String,
    pub error_code: Option<String>,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    pub backoff_level: i32,
    pub rate_limited_until: Option<String>,
    pub health_check_interval: i32,
    pub consecutive_use_count: i32,
    pub rate_limit_protection: bool,
    pub group_name: Option<String>,
    pub max_concurrent: Option<i32>,
    pub proxy_enabled: bool,
    pub display_name: Option<String>,
    pub default_model: Option<String>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    pub last_used_at: Option<String>,
    pub last_health_check_at: Option<String>,
    pub last_tested: Option<String>,
    pub provider_specific_data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProviderRequest {
    pub provider: String,
    pub auth_type: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub api_key: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub project_id: Option<String>,
    pub base_url: Option<String>,
    pub priority: Option<i32>,
    pub group_name: Option<String>,
    pub max_concurrent: Option<i32>,
    pub default_model: Option<String>,
    /// 列表里展示给用户的名字。自定义提供方场景必填，用于在已有内置 provider 中区分。
    pub display_name: Option<String>,
    /// API 协议（openai / anthropic / gemini / cloudflare / cohere）。仅自定义提供方使用。
    pub api_protocol: Option<String>,
    /// 用户填的 Provider ID（如 `acme-gateway`）。仅自定义提供方使用，写入 `provider_specific_data.customId`。
    pub custom_provider_id: Option<String>,
    pub provider_specific_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub machine_id: Option<String>,
    pub allowed_models: serde_json::Value,
    pub allowed_connections: serde_json::Value,
    pub allowed_endpoints: serde_json::Value,
    pub no_log: bool,
    pub auto_resolve: bool,
    pub is_active: bool,
    pub is_banned: bool,
    pub rate_limits: serde_json::Value,
    pub usage_limits: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub allowed_models: Option<serde_json::Value>,
    pub rate_limits: Option<serde_json::Value>,
    pub usage_limits: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    pub id: i64,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub connection_id: Option<String>,
    pub api_key_id: Option<String>,
    pub api_key_name: Option<String>,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_cache_read: i64,
    pub tokens_cache_creation: i64,
    pub tokens_reasoning: i64,
    pub service_tier: String,
    pub status: String,
    pub success: bool,
    pub error_code: Option<String>,
    pub latency_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub cost: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_requests: i64,
    pub total_tokens_input: i64,
    pub total_tokens_output: i64,
    pub total_cost: f64,
    pub avg_latency_ms: f64,
    pub success_rate: f64,
    pub by_provider: serde_json::Value,
    pub by_model: serde_json::Value,
    pub by_day: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    pub model: String,
    pub messages: serde_json::Value,
    pub stream: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
    pub api_key: Option<String>,
    pub source_format: String,
    pub extra: serde_json::Value,
}

/// 免费 Token 站点目录条目。
/// 请求与响应统一使用 camelCase，避免前后端命名不一致导致的字段丢失。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeTokenSite {
    pub id: String,
    pub name: String,
    pub home_url: String,
    pub apply_url: String,
    pub api_supported: bool,
    pub api_base: Option<String>,
    pub api_format: Option<String>,
    pub free_quota: String,
    pub region: String,
    pub requires_card: bool,
    pub requires_verify: bool,
    pub tags: serde_json::Value,
    pub note: Option<String>,
    pub provider_id: Option<String>,
    pub source: String,
    pub submitter: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// 用户提交的站点推荐。仅 `name` 为必填，其余字段留空时由后端补默认值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFreeTokenSiteRequest {
    pub name: String,
    #[serde(default)]
    pub home_url: Option<String>,
    #[serde(default)]
    pub apply_url: Option<String>,
    #[serde(default)]
    pub api_supported: Option<bool>,
    #[serde(default)]
    pub api_base: Option<String>,
    #[serde(default)]
    pub api_format: Option<String>,
    #[serde(default)]
    pub free_quota: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub requires_card: Option<bool>,
    #[serde(default)]
    pub requires_verify: Option<bool>,
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub submitter: Option<String>,
}

