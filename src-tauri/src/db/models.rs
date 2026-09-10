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
    pub provider_specific_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combo {
    pub id: String,
    pub name: String,
    pub data: ComboData,
    pub sort_order: i32,
    pub context_cache_protection: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboData {
    pub strategy: String,
    pub models: Vec<ComboStep>,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub system_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboStep {
    #[serde(rename = "modelStr")]
    pub model_str: String,
    pub provider: String,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub connection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateComboRequest {
    pub name: String,
    pub strategy: Option<String>,
    pub models: Option<Vec<ComboStep>>,
    pub config: Option<serde_json::Value>,
    pub system_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub machine_id: Option<String>,
    pub allowed_models: serde_json::Value,
    pub allowed_combos: serde_json::Value,
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
    pub allowed_combos: Option<serde_json::Value>,
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
    pub combo_strategy: Option<String>,
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
pub struct ResolvedComboTarget {
    pub step_id: String,
    pub model_str: String,
    pub provider: String,
    pub provider_id: String,
    pub connection_id: Option<String>,
    pub weight: f64,
    pub label: Option<String>,
    pub traffic_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingContext {
    pub usage_stats: Option<UsageStats>,
    pub quota_state: serde_json::Value,
    pub cost_catalog: serde_json::Value,
    pub latency_history: serde_json::Value,
    pub last_good_target: Option<String>,
    pub request_tokens: Option<i64>,
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

