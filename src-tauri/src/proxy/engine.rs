//! 代理引擎核心
//!
//! 负责入站请求的完整处理流程：API Key 鉴权 → 路由解析 → 上游执行（含重试与熔断）
//! → 用量记录。流式响应以归一化 SSE 流返回，由入站协议层决定包装方式。

use crate::db::models::{ProxyRequest, UsageEntry};
use crate::db::{core as db_core, providers as db_providers, usage as db_usage};
use crate::error::{AppError, Result};
use crate::providers::ProviderRegistry;
use crate::proxy::executor::{ExecutorFactory, ExecutorOutput};
use crate::proxy::retry::RetryPolicy;
use crate::proxy::sse::{SseStream, StreamUsage, UsageCallback};
use crate::AppState;
use std::sync::Arc;
use std::time::Instant;

/// 引擎处理结果。
/// 流式响应以归一化 SSE 流返回，由入站协议层（OpenAI/Anthropic）决定如何包装。
pub enum ProxyOutput {
    /// 非流式响应（OpenAI 格式 JSON）
    Response(serde_json::Value),
    /// 流式响应（归一化为 OpenAI chunk 格式的 SSE 流）
    Stream(SseStream),
}

/// 代理引擎
///
/// 核心代理逻辑入口，持有执行器工厂与重试策略，
/// 负责将请求路由到上游提供商并处理重试、熔断与用量记录。
/// 实现 Clone 以便 Tauri IPC 命令把引擎克隆出读锁后异步执行
///（parking_lot 读锁守卫非 Send，不能跨 await 持有）。
#[derive(Clone)]
pub struct ProxyEngine {
    /// 执行器工厂，按 API 格式创建对应执行器
    executor_factory: ExecutorFactory,
    /// 重试策略（最大 2 次重试，基础延迟 500ms）
    retry_policy: RetryPolicy,
}

impl ProxyEngine {
    /// 创建代理引擎
    ///
    /// 默认配置：最大重试 2 次，基础延迟 500 毫秒。
    pub fn new() -> Self {
        Self {
            executor_factory: ExecutorFactory::new(),
            retry_policy: RetryPolicy::new(2, 500),
        }
    }

    /// 处理入站代理请求
    ///
    /// 完整流程：API Key 鉴权 → 路由解析 → 带重试的上游执行 → 用量记录。
    /// 失败时记录错误用量条目。
    ///
    /// # 参数
    /// - `state`：应用全局状态
    /// - `request`：代理请求对象
    ///
    /// # 返回
    /// 成功时返回 `ProxyOutput`（流式或非流式），失败时返回 `AppError`
    pub async fn handle_request(
        &self,
        state: &Arc<AppState>,
        request: ProxyRequest,
    ) -> Result<ProxyOutput> {
        let start = Instant::now();
        let conn = db_core::get_conn(&state.db_pool)?;

        // API Key 鉴权
        if state.config.require_api_key {
            if let Some(ref api_key) = request.api_key {
                let key_record = db_api_keys::get_by_key(&conn, api_key)?;
                if key_record.is_none() {
                    return Err(AppError::Unauthorized("Invalid API key".into()));
                }
                let key_obj = key_record.unwrap();
                // 校验 API Key 的模型白名单
                if let Some(allowed) = key_obj.allowed_models.as_array() {
                    if !allowed.is_empty() {
                        let model_allowed = allowed.iter().any(|m| {
                            m.as_str().map(|s| request.model.starts_with(s)).unwrap_or(false)
                        });
                        if !model_allowed {
                            return Err(AppError::Unauthorized("Model not allowed for this API key".into()));
                        }
                    }
                }
            } else {
                return Err(AppError::Unauthorized("API key required".into()));
            }
        }

        // 路由解析：找到提供商定义、活跃连接与实际模型名
        let (provider_def, connection, model) =
            self.resolve_route(&conn, &state.provider_registry, &request, &state.encryption_key)?;

        let conn_obj = connection.ok_or_else(|| AppError::Provider("No active connection".into()))?;
        // 带重试与熔断的上游执行
        let result = self
            .handle_single_with_retry(state, &conn, &request, &provider_def, &conn_obj, &model, start)
            .await;

        // 失败时记录错误用量
        match &result {
            Ok(_) => {}
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                let entry = UsageEntry {
                    id: 0,
                    provider: Some(provider_def.id.clone()),
                    model: Some(model.clone()),
                    connection_id: Some(conn_obj.id.clone()),
                    api_key_id: request.api_key.clone(),
                    api_key_name: None,
                    tokens_input: 0,
                    tokens_output: 0,
                    tokens_cache_read: 0,
                    tokens_cache_creation: 0,
                    tokens_reasoning: 0,
                    service_tier: "standard".to_string(),
                    status: "error".to_string(),
                    success: false,
                    error_code: Some(format!("{}", e)),
                    latency_ms: Some(latency),
                    ttft_ms: None,
                    cost: 0.0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = db_usage::record(&conn, &entry);
            }
        }

        result
    }

    /// 路由解析：根据请求模型名找到对应的提供商定义与活跃连接
    ///
    /// 解析策略：
    /// 1. 优先通过注册表精确匹配模型名
    /// 2. 匹配失败时回退到配置了默认模型的活跃连接
    ///
    /// # 参数
    /// - `conn`：数据库连接
    /// - `registry`：提供商注册表
    /// - `request`：代理请求
    /// - `enc_key`：加密密钥（用于解密连接凭证）
    ///
    /// # 返回
    /// `(提供商定义, 活跃连接, 实际模型名)`
    fn resolve_route(
        &self,
        conn: &rusqlite::Connection,
        registry: &ProviderRegistry,
        request: &ProxyRequest,
        enc_key: &[u8],
    ) -> Result<(crate::providers::types::ProviderDef, Option<crate::db::models::ProviderConnection>, String)> {
        // 优先通过注册表精确匹配
        if let Some((provider_id, def, model)) = registry.resolve_model_provider(&request.model) {
            let connections = db_providers::list_by_provider(conn, &provider_id, enc_key)?;
            // 取第一个活跃连接
            let connection = connections.into_iter().next();
            return Ok((def.clone(), connection, model));
        }

        // 模型名无法解析（如 Claude Code 发送的 claude-sonnet-4 等原生模型名）：
        // 回退到配置了默认模型的活跃连接（借鉴 relay-rs 的默认模型回退）
        for def in registry.list() {
            let connections = db_providers::list_by_provider(conn, &def.id, enc_key)?;
            if let Some(c) = connections.iter().find(|c| {
                c.default_model.as_deref().is_some_and(|m| !m.is_empty())
            }) {
                let model = c.default_model.clone().unwrap_or_default();
                log::warn!(
                    "Model '{}' cannot be resolved, falling back to default model '{}/{}' (connection: {})",
                    request.model,
                    def.id,
                    model,
                    c.name
                );
                return Ok((def.clone(), Some(c.clone()), model));
            }
        }

        Err(AppError::Routing(format!("Cannot resolve model: {}", request.model)))
    }

    /// 带重试与熔断的单连接请求处理
    ///
    /// 每次尝试前检查熔断器状态，执行后根据结果记录成功/失败。
    /// 对 5xx 和 429 错误按重试策略进行指数退避重试。
    ///
    /// # 参数
    /// - `state`：应用全局状态
    /// - `conn`：数据库连接
    /// - `request`：代理请求
    /// - `def`：提供商定义
    /// - `connection`：提供商连接
    /// - `model`：实际模型名
    /// - `start`：请求起始时间
    ///
    /// # 返回
    /// 成功时返回 `ProxyOutput`，失败时返回 `AppError`
    async fn handle_single_with_retry(
        &self,
        state: &Arc<AppState>,
        conn: &rusqlite::Connection,
        request: &ProxyRequest,
        def: &crate::providers::types::ProviderDef,
        connection: &crate::db::models::ProviderConnection,
        model: &str,
        start: Instant,
    ) -> Result<ProxyOutput> {
        // 按提供商格式创建执行器
        let executor = self.executor_factory.create(def);
        let mut last_err = None;

        // 重试循环
        for attempt in 0..=self.retry_policy.max_retries() {
            // 熔断器名称：provider_id:connection_id
            let breaker_name = format!("{}:{}", def.id, connection.id);
            // 熔断器打开时跳过执行
            if !state.resilience_manager.is_available(&breaker_name) {
                last_err = Some(AppError::Provider(format!("Circuit breaker open for {}", breaker_name)));
                break;
            }

            // 构造流式用量落库回调（流结束时由 sse.rs 调用）
            let cb_pool = state.db_pool.clone();
            let cb_provider = def.id.clone();
            let cb_model = model.to_string();
            let cb_conn = connection.id.clone();
            let cb_api_key = request.api_key.clone();
            let cb_start = start;
            let usage_cb: UsageCallback = Arc::new(move |usage: StreamUsage| {
                let Ok(c) = db_core::get_conn(&cb_pool) else { return };
                let latency = cb_start.elapsed().as_millis() as i64;
                let entry = UsageEntry {
                    id: 0,
                    provider: Some(cb_provider.clone()),
                    model: Some(cb_model.clone()),
                    connection_id: Some(cb_conn.clone()),
                    api_key_id: cb_api_key.clone(),
                    api_key_name: None,
                    tokens_input: usage.input_tokens,
                    tokens_output: usage.output_tokens,
                    tokens_cache_read: 0,
                    tokens_cache_creation: 0,
                    tokens_reasoning: 0,
                    service_tier: "standard".to_string(),
                    status: "success".to_string(),
                    success: true,
                    error_code: None,
                    latency_ms: Some(latency),
                    ttft_ms: None,
                    cost: 0.0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = db_usage::record(&c, &entry);
            });

            match executor.execute(state, request, def, connection, model, Some(usage_cb)).await {
                Ok(ExecutorOutput::Json(body)) => {
                    // 非流式成功：提取用量、记录成功、写入用量条目
                    let (tokens_in, tokens_out, cost) =
                        extract_usage_from_response(&body, def.api_format.as_str());
                    let latency = start.elapsed().as_millis() as i64;
                    let _ = db_providers::reset_backoff(conn, &connection.id);
                    let _ = db_providers::increment_usage(conn, &connection.id);
                    state.resilience_manager.record_success(&breaker_name);
                    let entry = UsageEntry {
                        id: 0,
                        provider: Some(def.id.clone()),
                        model: Some(model.to_string()),
                        connection_id: Some(connection.id.clone()),
                        api_key_id: request.api_key.clone(),
                        api_key_name: None,
                        tokens_input: tokens_in,
                        tokens_output: tokens_out,
                        tokens_cache_read: 0,
                        tokens_cache_creation: 0,
                        tokens_reasoning: 0,
                        service_tier: "standard".to_string(),
                        status: "success".to_string(),
                        success: true,
                        error_code: None,
                        latency_ms: Some(latency),
                        ttft_ms: None,
                        cost,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = db_usage::record(conn, &entry);

                    return Ok(ProxyOutput::Response(body));
                }
                Ok(ExecutorOutput::Stream(stream)) => {
                    // 流式响应直接透传，token 用量由回调在流结束时写库
                    let _ = db_providers::reset_backoff(conn, &connection.id);
                    let _ = db_providers::increment_usage(conn, &connection.id);
                    state.resilience_manager.record_success(&breaker_name);

                    return Ok(ProxyOutput::Stream(stream));
                }
                Ok(ExecutorOutput::UpstreamError { status, body }) => {
                    // 上游返回非 2xx
                    let status_code = status;
                    if !self.retry_policy.should_retry(attempt, Some(status_code)) {
                        // 不可重试：429 时增加退避时间，记录熔断失败
                        if status_code == 429 {
                            let retry_after = chrono::Utc::now() + chrono::Duration::seconds(60);
                            let _ = db_providers::increment_backoff(conn, &connection.id, Some(retry_after.to_rfc3339().as_str()));
                        }
                        state.resilience_manager.record_failure(&breaker_name);
                        return Err(AppError::Provider(format!("Provider returned {}: {}", status_code, body)));
                    }

                    // 可重试：指数退避等待后继续
                    let delay = self.retry_policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(AppError::Provider(format!("Provider returned {} (attempt {}), retrying", status_code, attempt + 1)));
                }
                Err(e) => {
                    // 网络错误或其他异常
                    state.resilience_manager.record_failure(&breaker_name);
                    if !self.retry_policy.should_retry(attempt, None) {
                        return Err(e);
                    }
                    // 指数退避等待后继续
                    let delay = self.retry_policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| AppError::Provider("All retries exhausted".into())))
    }
}

/// 从响应体中提取用量信息（输入/输出 token 数与费用）
///
/// 根据不同的 API 格式从响应 JSON 中提取对应的用量字段：
/// - `anthropic`：`usage.input_tokens` / `usage.output_tokens`
/// - `gemini`：`usageMetadata.promptTokenCount` / `usageMetadata.candidatesTokenCount`
/// - 其他（OpenAI）：`usage.prompt_tokens` / `usage.completion_tokens`
///
/// # 参数
/// - `body`：响应 JSON 体
/// - `api_format`：API 格式标识
///
/// # 返回
/// `(输入 token 数, 输出 token 数, 费用)`，费用目前固定返回 0.0
fn extract_usage_from_response(body: &serde_json::Value, api_format: &str) -> (i64, i64, f64) {
    let usage = body.get("usage");
    // 优先按原始格式提取，回退到 OpenAI 格式（translator 可能已转换）
    let (tokens_in, tokens_out) = match api_format {
        "anthropic" => {
            let input = usage
                .and_then(|u| u.get("input_tokens"))
                .or_else(|| usage.and_then(|u| u.get("prompt_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = usage
                .and_then(|u| u.get("output_tokens"))
                .or_else(|| usage.and_then(|u| u.get("completion_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output)
        }
        "gemini" => {
            let meta = body.get("usageMetadata");
            let input = meta
                .and_then(|u| u.get("promptTokenCount"))
                .or_else(|| usage.and_then(|u| u.get("prompt_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = meta
                .and_then(|u| u.get("candidatesTokenCount"))
                .or_else(|| usage.and_then(|u| u.get("completion_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output)
        }
        _ => {
            let input = usage
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = usage
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output)
        }
    };

    // 费用暂未实现，固定返回 0.0
    (tokens_in, tokens_out, 0.0)
}

use crate::db::api_keys as db_api_keys;
