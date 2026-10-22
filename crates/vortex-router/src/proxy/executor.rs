//! 上游执行器
//!
//! 负责不同 API 格式的请求构建和发送。多级超时保护（借鉴 relay-rs）：
//!
//! - 连接超时 10s（HTTP 客户端全局配置）
//! - 流式首字节 60s（发出请求到收到响应头，`tokio::time::timeout` 包裹）
//! - 非流式整体 300s（awc 请求级超时，含读取响应体）
//! - 流式逐块 120s（在 sse.rs 中按次计时，不限制总时长）
//!
//! 请求不会因全局总超时被中途杀掉，长流式生成不受影响。

use vortex_store::db::models::{ProxyRequest, ProviderConnection};
use crate::context::EngineContext;
use crate::providers::types::ProviderDef;
use vortex_store::error::{AppError, Result};
use crate::proxy::sse::{self, SseStream, StreamUsage, UpstreamStream, UsageCallback};
use crate::proxy::web_bridge::{WebChatBridge, WebChunkKind};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use std::sync::OnceLock;

static DSML_REGISTRY: OnceLock<Arc<ai_web_provider::dsml::registry::InvokeRegistry>> = OnceLock::new();
static DSML_CONFIG: OnceLock<ai_web_provider::dsml::DsmlConfig> = OnceLock::new();
static DSML_METRICS: OnceLock<Arc<ai_web_provider::dsml::DsmlMetrics>> = OnceLock::new();

fn dsml_registry() -> &'static Arc<ai_web_provider::dsml::registry::InvokeRegistry> {
    DSML_REGISTRY.get_or_init(|| {
        Arc::new(ai_web_provider::dsml::registry::InvokeRegistry::new(300))
    })
}
fn dsml_config() -> &'static ai_web_provider::dsml::DsmlConfig {
    DSML_CONFIG.get_or_init(ai_web_provider::dsml::DsmlConfig::load)
}
fn dsml_metrics() -> &'static Arc<ai_web_provider::dsml::DsmlMetrics> {
    DSML_METRICS.get_or_init(ai_web_provider::dsml::DsmlMetrics::register)
}

fn register_web_tool_calls(
    response: &Value,
    request_id: &str,
    registry: &ai_web_provider::dsml::registry::InvokeRegistry,
) {
    if let Some(tool_calls) = response["choices"][0]["message"]["tool_calls"].as_array() {
        for tc in tool_calls {
            let id = tc["id"].as_str().unwrap_or("");
            let name = tc["function"]["name"].as_str().unwrap_or("");
            registry.register(
                request_id,
                id,
                ai_web_provider::dsml::types::Invoke {
                    name: name.to_string(),
                    parameters: vec![],
                },
            );
        }
    }
}

/// 提取 tools 白名单；DSML_ALWAYS_PARSE=true 时即使无 tools 字段也返回空白名单（接受所有工具名）。
fn extract_tools_whitelist_with_fallback(body: &Value) -> Option<Vec<String>> {
    if let Some(w) = ai_web_provider::dsml::extract_tools_whitelist(body) {
        return Some(w);
    }
    if std::env::var("DSML_ALWAYS_PARSE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
    {
        Some(vec![])
    } else {
        None
    }
}

/// 补全非流式响应中 tool_calls 缺失的 required 参数。
fn fill_required_params(response: &mut Value, tools: Option<&Value>) {
    let Some(tools_arr) = tools.and_then(|t| t.as_array()) else {
        return;
    };
    let mut required_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for tool in tools_arr {
        let name = tool
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str());
        let required = tool
            .get("function")
            .and_then(|f| f.get("parameters"))
            .and_then(|p| p.get("required"))
            .and_then(|r| r.as_array());
        if let (Some(name), Some(required)) = (name, required) {
            let params: Vec<String> =
                required.iter().filter_map(|r| r.as_str().map(String::from)).collect();
            required_map.insert(name.to_string(), params);
        }
    }
    if let Some(tool_calls) = response
        .pointer_mut("/choices/0/message/tool_calls")
        .and_then(|v| v.as_array_mut())
    {
        for tc in tool_calls {
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(String::from);
            if let Some(name) = name {
                if let Some(required) = required_map.get(&name) {
                    let args_str = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}");
                    let mut args: Value =
                        serde_json::from_str(args_str).unwrap_or(json!({}));
                    if let Some(args_map) = args.as_object_mut() {
                        for param in required {
                            if !args_map.contains_key(param) {
                                args_map.insert(
                                    param.clone(),
                                    json!(format!("Auto-filled by proxy: {name}")),
                                );
                            }
                        }
                    }
                    if let Some(func) = tc.get_mut("function").and_then(|f| f.as_object_mut()) {
                        func.insert(
                            "arguments".to_string(),
                            json!(serde_json::to_string(&args).unwrap_or_default()),
                        );
                    }
                }
            }
        }
    }
}

/// 补全流式 arguments chunk 中缺失的 required 参数。
fn fill_arguments_chunk(arguments: &str, tool_name: &str, tools: Option<&Value>) -> String {
    let Some(tools_arr) = tools.and_then(|t| t.as_array()) else {
        return arguments.to_string();
    };
    let tool_def = tools_arr.iter().find(|t| {
        t.get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            == Some(tool_name)
    });
    let Some(tool_def) = tool_def else {
        return arguments.to_string();
    };
    let required = tool_def
        .get("function")
        .and_then(|f| f.get("parameters"))
        .and_then(|p| p.get("required"))
        .and_then(|r| r.as_array());
    let Some(required) = required else {
        return arguments.to_string();
    };
    let mut args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
    if let Some(args_map) = args.as_object_mut() {
        for param in required {
            if let Some(pname) = param.as_str() {
                if !args_map.contains_key(pname) {
                    args_map.insert(
                        pname.to_string(),
                        json!(format!("Auto-filled by proxy: {tool_name}")),
                    );
                }
            }
        }
    }
    serde_json::to_string(&args).unwrap_or_else(|_| arguments.to_string())
}

/// 格式化完整错误链（遍历 `std::error::Error::source()`）。
///
/// awc 的顶层错误（如 "error sending request for url"）往往不包含
/// 根因（DNS 解析失败、TLS 握手失败、连接拒绝等），需要沿 source 链逐层展开。
fn format_error_chain(e: &dyn std::error::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut current = e.source();
    while let Some(source) = current {
        let msg = source.to_string();
        // 避免重复消息（有些层级的 to_string 会包含下一层的信息）
        if !parts.iter().any(|p| p == &msg) {
            parts.push(msg);
        }
        current = source.source();
    }
    parts.join(" → ")
}

/// 非流式请求整体超时（含等待响应与读取 body）
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// 流式请求建立超时（发出请求到收到响应头）
const FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(60);

/// 执行器输出。上游非 2xx 以 UpstreamError 返回，由引擎统一决定重试。
pub enum ExecutorOutput {
    /// 非流式成功响应（已统一为 OpenAI 格式）
    Json(Value),
    /// 流式成功响应（已归一化为 OpenAI chunk 格式的 SSE 流）
    Stream(SseStream),
    /// 上游返回非 2xx 状态码
    UpstreamError { status: u16, body: String },
}

/// 执行器工厂
///
/// 根据提供商的 API 格式创建对应的执行器实例。
#[derive(Clone)]
pub struct ExecutorFactory;

impl ExecutorFactory {
    /// 创建执行器工厂
    pub fn new() -> Self {
        Self
    }

    /// 根据提供商定义创建对应格式的执行器
    ///
    /// # 参数
    /// - `def`：提供商定义
    /// - `ctx`：引擎上下文（web 格式执行器从中取网页版平台桥接）
    ///
    /// # 返回
    /// 按格式匹配的执行器：`anthropic` → AnthropicExecutor，
    /// `gemini` → GeminiExecutor，`web` → WebExecutor（无桥接时为占位实例，
    /// 执行时报错），其他 → OpenAIExecutor
    pub fn create(&self, def: &ProviderDef, ctx: &dyn EngineContext) -> Box<dyn ProviderExecutor + Send + Sync> {
        match def.api_format.as_str() {
            "anthropic" => Box::new(AnthropicExecutor),
            "gemini" => Box::new(GeminiExecutor),
            "web" => Box::new(WebExecutor::new(ctx.web_bridge())),
            _ => Box::new(OpenAIExecutor),
        }
    }
}

/// 提供商执行器 trait
///
/// 定义向上游 AI 提供商发送请求的统一接口，
/// 不同 API 格式的执行器各自实现此 trait。
#[async_trait::async_trait(?Send)]
pub trait ProviderExecutor: Send + Sync {
    /// 执行上游请求
    ///
    /// # 参数
    /// - `state`：应用全局状态
    /// - `request`：代理请求
    /// - `def`：提供商定义
    /// - `connection`：提供商连接
    /// - `model`：实际模型名
    ///
    /// # 返回
    /// 执行器输出（JSON / 流 / 上游错误）
    async fn execute(
        &self,
        client: &awc::Client,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
        usage_cb: Option<UsageCallback>,
    ) -> Result<ExecutorOutput>;
}

/// 发送请求并应用首字节超时（防止上游迟迟不返回响应头）
///
/// 泛型 R 由调用点自动推导，避免显式命名 awc::ClientResponse 的 body 类型参数。
async fn send_with_first_byte_timeout<R>(
    fut: impl std::future::Future<Output = std::result::Result<R, awc::error::SendRequestError>>,
) -> std::result::Result<R, AppError> {
    tokio::time::timeout(FIRST_BYTE_TIMEOUT, fut)
        .await
        .map_err(|_| {
            AppError::Provider(format!(
                "上游响应超时（{} 秒未收到响应头）",
                FIRST_BYTE_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| {
            let chain = format_error_chain(&e);
            log::error!("上游请求失败: {}", chain);
            AppError::Provider(format!("Request failed: {}", chain))
        })
}

/// 读取上游错误响应体，构造 UpstreamError
async fn upstream_error<S>(mut response: awc::ClientResponse<S>) -> ExecutorOutput
where
    S: futures::Stream<Item = std::result::Result<actix_web::web::Bytes, awc::error::PayloadError>> + Unpin + 'static,
{
    let status = response.status().as_u16();
    let bytes = response.body().await.unwrap_or_default();
    let body = String::from_utf8_lossy(&bytes).into_owned();
    ExecutorOutput::UpstreamError { status, body }
}

/// 解析上游 400 错误响应，提取被拒绝的采样参数名。
///
/// 部分提供商对特定模型限制采样参数（如 Kimi moonshot-v1-auto 只允许 temperature=1），
/// 上游返回 400 并在错误体中标注 `param` 字段。此函数提取需移除的参数名列表，
/// 供执行器在重试时从请求体中剔除。
fn parse_invalid_params(error_body: &str) -> Vec<String> {
    let mut params = Vec::new();

    if let Ok(json) = serde_json::from_str::<Value>(error_body) {
        if let Some(param) = json
            .get("error")
            .and_then(|e| e.get("param"))
            .and_then(|p| p.as_str())
            .filter(|s| !s.is_empty())
        {
            params.push(param.to_string());
        }
    }

    if params.is_empty() {
        let lower = error_body.to_lowercase();
        for param in &["temperature", "top_p", "top_k", "frequency_penalty", "presence_penalty"] {
            if lower.contains(param) {
                params.push(param.to_string());
            }
        }
    }

    params
}

/// 读取非流式 JSON 响应体，应用整体超时
async fn read_json_response<S>(mut response: awc::ClientResponse<S>) -> Result<Value>
where
    S: futures::Stream<Item = std::result::Result<actix_web::web::Bytes, awc::error::PayloadError>> + Unpin + 'static,
{
    let bytes = tokio::time::timeout(REQUEST_TIMEOUT, response.body())
        .await
        .map_err(|_| {
            AppError::Provider(format!(
                "上游响应体读取超时（{} 秒）",
                REQUEST_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| AppError::Provider(format!("Failed to read response body: {}", format_error_chain(&e))))?;
    serde_json::from_slice::<Value>(&bytes)
        .map_err(|e| AppError::Provider(format!("Failed to parse response JSON: {}", e)))
}

/// 拼接上游 URL 并做版本段归一化。
///
/// 连接级 baseUrl 通常已含版本前缀（如 `https://host/v1`、`https://host/v4`），
/// 而提供商定义的 `chat_path` 也以 `/v1` 开头，直接相加会得到 `/v4/v1/chat/completions`（上游 404）。
/// 此处：当 baseUrl 以 `/vN` 结尾且 path 以 `vM/` 开头时去掉 path 中的版本段，其余情况原样拼接。
fn build_upstream_url(base_url: &str, chat_path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let path = chat_path.strip_prefix('/').unwrap_or(chat_path);
    let path = {
        let base_ver = base.rsplit_once('/').map(|(_, v)| v).unwrap_or("");
        let is_base_ver = is_version_segment(base_ver);
        if is_base_ver {
            if let Some(slash) = path.find('/') {
                let path_ver = &path[..slash];
                if is_version_segment(path_ver) {
                    &path[slash + 1..]
                } else {
                    path
                }
            } else {
                path
            }
        } else {
            path
        }
    };
    format!("{}/{}", base, path)
}

/// 判断字符串是否为版本段（`v1`、`v2`、`v4` 等）
fn is_version_segment(s: &str) -> bool {
    s.starts_with('v') && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit())
}

/// OpenAI 格式执行器
///
/// 直接使用 OpenAI chat/completions 接口格式，请求与响应无需转换。
pub struct OpenAIExecutor;

#[async_trait::async_trait(?Send)]
impl ProviderExecutor for OpenAIExecutor {
    /// 执行 OpenAI 格式请求
    ///
    /// 构建请求体（注入 model、temperature、max_tokens 等参数），
    /// 设置 Bearer 认证，发送请求并处理流式/非流式响应。
    async fn execute(
        &self,
        client: &awc::Client,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
        usage_cb: Option<UsageCallback>,
    ) -> Result<ExecutorOutput> {
        // 优先使用连接级 baseUrl，回退到提供商定义的默认值
        let base_url = connection
            .provider_specific_data
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or(&def.base_url);

        // 连接级接口路径覆盖：允许自定义提供方（如 z.ai Coding Plan）使用非 /v1 路径。
        // 未设置时回退到提供商定义里的默认 chat_path。
        let chat_path = connection
            .provider_specific_data
            .get("chatPath")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&def.chat_path);

        let url = build_upstream_url(base_url, chat_path);

        log::info!("上游请求: POST {} (model: {}, stream: {})", url, model, request.stream);

        // 构建请求体：在原始消息基础上注入模型与采样参数
        let mut body = request.messages.clone();
        if let Some(obj) = body.as_object_mut() {
            obj.insert("model".to_string(), json!(model));
            if let Some(temp) = request.temperature {
                obj.insert("temperature".to_string(), json!(temp));
            }
            if let Some(max_tokens) = request.max_tokens {
                obj.insert("max_tokens".to_string(), json!(max_tokens));
            }
            if let Some(top_p) = request.top_p {
                obj.insert("top_p".to_string(), json!(top_p));
            }
            obj.insert("stream".to_string(), json!(request.stream));
            // 流式请求：要求上游在流末尾返回 usage 收尾块
            if request.stream {
                obj.insert("stream_options".to_string(), json!({"include_usage": true}));
            }
        }

        // 发送请求（含参数自修复：部分模型如 Kimi 限制采样参数，
        // 上游 400 时自动移除被拒绝的参数重试一次）
        let mut param_fix_attempted = false;
        loop {
            let mut req = client.post(&url);
            if let Some(ref api_key) = connection.api_key {
                req = req.insert_header(("Authorization", format!("Bearer {}", api_key)));
            }

            let response = send_with_first_byte_timeout(req.send_json(&body)).await?;
            let status = response.status().as_u16();

            if status == 400 && !param_fix_attempted {
                let err_output = upstream_error(response).await;
                if let ExecutorOutput::UpstreamError { body: ref err_body, .. } = err_output {
                    let invalid = parse_invalid_params(err_body);
                    if !invalid.is_empty() {
                        log::info!("参数自修复：上游拒绝 {}，移除后重试", invalid.join(", "));
                        if let Some(obj) = body.as_object_mut() {
                            for p in &invalid {
                                obj.remove(p);
                            }
                        }
                        param_fix_attempted = true;
                        continue;
                    }
                }
                return Ok(err_output);
            }

            if status >= 400 {
                return Ok(upstream_error(response).await);
            }

            if request.stream {
                let stream: UpstreamStream = Box::pin(response);
                return Ok(ExecutorOutput::Stream(
                    sse::stream_sse_response(stream, "openai", usage_cb).await,
                ));
            } else {
                let json_body = read_json_response(response).await?;
                if json_body.get("error").is_some() {
                    let err_str = serde_json::to_string(&json_body).unwrap_or_default();
                    log::warn!("openai: HTTP 200 但响应体含 error 字段: {}", err_str);
                    return Ok(ExecutorOutput::UpstreamError { status: 200, body: json_body.to_string() });
                }
                return Ok(ExecutorOutput::Json(json_body));
            }
        }
    }
}

/// Anthropic 格式执行器
///
/// 将 OpenAI 格式请求转换为 Anthropic Messages 格式发送，
/// 并将非流式响应转回 OpenAI 格式。
pub struct AnthropicExecutor;

#[async_trait::async_trait(?Send)]
impl ProviderExecutor for AnthropicExecutor {
    /// 执行 Anthropic 格式请求
    ///
    /// 将 OpenAI 消息/工具定义转换为 Anthropic 格式，设置 x-api-key 认证，
    /// 发送请求。非流式响应通过转换器转回 OpenAI 格式。
    async fn execute(
        &self,
        client: &awc::Client,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
        usage_cb: Option<UsageCallback>,
    ) -> Result<ExecutorOutput> {
        let base_url = connection
            .provider_specific_data
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or(&def.base_url);
        let url = build_upstream_url(base_url, &def.chat_path);

        // OpenAI 消息 → Anthropic 消息（含 tool_calls → tool_use 全链路转换）
        let (system, messages) = crate::translator::openai_messages_to_anthropic(&request.messages);

        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "stream": request.stream,
        });

        // 设置 system 提示
        if let Some(sys) = system {
            body["system"] = sys;
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(t) = request.top_p {
            body["top_p"] = json!(t);
        }
        // 转换停止序列
        if let Some(stops) = request.messages.get("stop").and_then(|s| s.as_array()) {
            body["stop_sequences"] = json!(stops);
        }
        // 转换工具定义与工具选择
        if let Some(tools) = request
            .messages
            .get("tools")
            .and_then(crate::translator::openai_tools_to_anthropic)
        {
            body["tools"] = tools;
            if let Some(tc) = request
                .messages
                .get("tool_choice")
                .and_then(crate::translator::openai_tool_choice_to_anthropic)
            {
                body["tool_choice"] = tc;
            }
        }

        let api_key = connection
            .api_key
            .as_deref()
            .ok_or_else(|| AppError::Unauthorized("API key required".into()))?;

        // 发送请求（含参数自修复重试）
        let mut param_fix_attempted = false;
        loop {
            let req = client
                .post(&url)
                .insert_header(("x-api-key", api_key))
                .insert_header(("anthropic-version", "2023-06-01"))
                .insert_header(("content-type", "application/json"));

            let response = send_with_first_byte_timeout(req.send_json(&body)).await?;
            let status = response.status().as_u16();

            if status == 400 && !param_fix_attempted {
                let err_output = upstream_error(response).await;
                if let ExecutorOutput::UpstreamError { body: ref err_body, .. } = err_output {
                    let invalid = parse_invalid_params(err_body);
                    if !invalid.is_empty() {
                        log::info!("参数自修复：上游拒绝 {}，移除后重试", invalid.join(", "));
                        if let Some(obj) = body.as_object_mut() {
                            for p in &invalid {
                                obj.remove(p);
                            }
                        }
                        param_fix_attempted = true;
                        continue;
                    }
                }
                return Ok(err_output);
            }

            if status >= 400 {
                return Ok(upstream_error(response).await);
            }

            if request.stream {
                let stream: UpstreamStream = Box::pin(response);
                return Ok(ExecutorOutput::Stream(
                    sse::stream_sse_response(stream, "anthropic", usage_cb).await,
                ));
            } else {
                let json_body = read_json_response(response).await?;
                if json_body.get("error").is_some() {
                    let err_str = serde_json::to_string(&json_body).unwrap_or_default();
                    log::warn!("anthropic: HTTP 200 但响应体含 error 字段: {}", err_str);
                    return Ok(ExecutorOutput::UpstreamError { status: 200, body: json_body.to_string() });
                }
                let openai_response = crate::translator::anthropic_to_openai_response(&json_body, model);
                return Ok(ExecutorOutput::Json(openai_response));
            }
        }
    }
}

/// Gemini 格式执行器
///
/// 将 OpenAI 格式请求转换为 Gemini generateContent 格式发送，
/// 并将非流式响应转回 OpenAI 格式。API key 通过 URL 查询参数传递。
pub struct GeminiExecutor;

#[async_trait::async_trait(?Send)]
impl ProviderExecutor for GeminiExecutor {
    /// 执行 Gemini 格式请求
    ///
    /// 将 OpenAI 消息转换为 Gemini contents 格式（system → systemInstruction，
    /// assistant → model 角色），设置 generationConfig，通过 URL 参数传递 API key。
    async fn execute(
        &self,
        client: &awc::Client,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
        usage_cb: Option<UsageCallback>,
    ) -> Result<ExecutorOutput> {
        let api_key = connection
            .api_key
            .as_deref()
            .ok_or_else(|| AppError::Unauthorized("API key required".into()))?;

        // 流式使用 streamGenerateContent，非流式使用 generateContent
        let chat_path = if request.stream {
            def.chat_path
                .replace("{model}", model)
                .replace("generateContent", "streamGenerateContent")
        } else {
            def.chat_path.replace("{model}", model)
        };
        // Gemini 通过 URL 查询参数传递 API key
        let url = format!("{}{}?key={}", def.base_url, chat_path, api_key);

        let mut system_instruction: Option<Value> = None;
        let mut contents = vec![];

        // 转换 OpenAI 消息为 Gemini contents 格式
        if let Some(obj) = request.messages.as_object()
            && let Some(msgs) = obj.get("messages").and_then(|m| m.as_array()) {
                for msg in msgs {
                    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    let content_str = crate::translator::extract_text_content(msg.get("content"));

                    if role == "system" {
                        // system 消息 → systemInstruction
                        system_instruction = Some(json!({"parts": [{"text": content_str}]}));
                    } else {
                        // assistant → model，其他 → user
                        let gemini_role = if role == "assistant" { "model" } else { "user" };
                        contents.push(json!({
                            "role": gemini_role,
                            "parts": [{"text": content_str}]
                        }));
                    }
                }
            }

        // 构建生成配置
        let mut generation_config = json!({
            "temperature": request.temperature.unwrap_or(1.0),
            "maxOutputTokens": request.max_tokens.unwrap_or(8192),
        });
        if let Some(t) = request.top_p {
            generation_config["topP"] = json!(t);
        }

        let mut body = json!({
            "contents": contents,
            "generationConfig": generation_config,
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = sys;
        }

        // 发送请求（含参数自修复重试，参数在 generationConfig 内）
        let mut param_fix_attempted = false;
        loop {
            let req = client.post(&url).insert_header(("content-type", "application/json"));

            let response = send_with_first_byte_timeout(req.send_json(&body)).await?;
            let status = response.status().as_u16();

            if status == 400 && !param_fix_attempted {
                let err_output = upstream_error(response).await;
                if let ExecutorOutput::UpstreamError { body: ref err_body, .. } = err_output {
                    let invalid = parse_invalid_params(err_body);
                    if !invalid.is_empty() {
                        log::info!("参数自修复：上游拒绝 {}，移除后重试", invalid.join(", "));
                        if let Some(gc) = body.get_mut("generationConfig").and_then(|g| g.as_object_mut()) {
                            for p in &invalid {
                                let gemini_key = match p.as_str() {
                                    "temperature" => "temperature",
                                    "top_p" => "topP",
                                    "max_tokens" => "maxOutputTokens",
                                    other => other,
                                };
                                gc.remove(gemini_key);
                            }
                        }
                        param_fix_attempted = true;
                        continue;
                    }
                }
                return Ok(err_output);
            }

            if status >= 400 {
                return Ok(upstream_error(response).await);
            }

            if request.stream {
                let stream: UpstreamStream = Box::pin(response);
                return Ok(ExecutorOutput::Stream(
                    sse::stream_sse_response(stream, "gemini", usage_cb).await,
                ));
            } else {
                let json_body = read_json_response(response).await?;
                if json_body.get("error").is_some() {
                    let err_str = serde_json::to_string(&json_body).unwrap_or_default();
                    log::warn!("gemini: HTTP 200 但响应体含 error 字段: {}", err_str);
                    return Ok(ExecutorOutput::UpstreamError { status: 200, body: json_body.to_string() });
                }
                let openai_response = crate::translator::gemini_to_openai_response(&json_body, model);
                return Ok(ExecutorOutput::Json(openai_response));
            }
        }
    }
}

/// Web（浏览器自动化）格式执行器
///
/// 不发 HTTP 请求：把 OpenAI 请求转换为对网页版平台 worker 的 ask 命令
/// （经 [`WebChatBridge`]），回复组装为 OpenAI chat.completion 格式
/// （流式为 chat.completion.chunk SSE）。
///
/// 会话语义：浏览器聊天页自带上下文，只下发 system 前缀 + 最后一条
/// user 消息；thinking 放 `message.reasoning_content` / `delta.reasoning_content`。
pub struct WebExecutor {
    bridge: Option<Arc<dyn WebChatBridge>>,
}

impl WebExecutor {
    pub fn new(bridge: Option<Arc<dyn WebChatBridge>>) -> Self {
        Self { bridge }
    }
}

/// 从 OpenAI 请求体提取下发问题：system（含 developer）拼接 +
/// 最后一条 user 消息。assistant/tool 历史不回放（浏览器会话自带上下文）。
fn web_question(body: &Value) -> Option<String> {
    let msgs = body.get("messages")?.as_array()?;
    let content_text = |v: Option<&Value>| -> String {
        match v {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                        p.get("text").and_then(|t| t.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        }
    };
    let mut system: Vec<String> = Vec::new();
    let mut last_user = String::new();
    for m in msgs {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let text = content_text(m.get("content"));
        if text.is_empty() {
            continue;
        }
        match role {
            "system" | "developer" => system.push(text),
            "user" => last_user = text,
            _ => {}
        }
    }
    let sys = system.join("\n\n");
    let q = if sys.is_empty() {
        last_user
    } else if last_user.is_empty() {
        sys
    } else {
        format!("{sys}\n\n{last_user}")
    };
    if q.trim().is_empty() {
        None
    } else {
        Some(q)
    }
}

/// 生成 OpenAI 风格响应 id（时间戳 + 计数，无 uuid 依赖足够唯一）
fn web_completion_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("chatcmpl-web-{secs:x}{n:x}")
}

/// web 流式任务的 SSE 单帧构造（chat.completion.chunk）
fn web_chunk_frame(id: &str, created: u64, model: &str, delta: Value, finish: Option<&str>) -> actix_web::web::Bytes {
    let mut choice = json!({"index": 0, "delta": delta});
    choice["finish_reason"] = match finish {
        Some(f) => json!(f),
        None => Value::Null,
    };
    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [choice],
    });
    actix_web::web::Bytes::from(format!("data: {chunk}\n\n"))
}

#[async_trait::async_trait(?Send)]
impl ProviderExecutor for WebExecutor {
    async fn execute(
        &self,
        _client: &awc::Client,
        request: &ProxyRequest,
        def: &ProviderDef,
        _connection: &ProviderConnection,
        _model: &str,
        usage_cb: Option<UsageCallback>,
    ) -> Result<ExecutorOutput> {
        use futures::{SinkExt, StreamExt};

        let Some(bridge) = self.bridge.as_ref() else {
            return Ok(ExecutorOutput::UpstreamError {
                status: 503,
                body: "网页版提供商未启用（web bridge 缺失）".into(),
            });
        };
        // ProviderDef.id 形如 "web-doubao" → ai-web-provider 平台名 "doubao"。
        // worker 未启动时由 bridge 按需拉起（未授权会继续拉起 CDP 登录窗口），
        // 这里不再提前返回 503。
        let platform = def.id.strip_prefix("web-").unwrap_or(&def.id).to_string();
        let Some(question) = web_question(&request.messages) else {
            return Ok(ExecutorOutput::UpstreamError {
                status: 400,
                body: "messages 中没有可用的用户问题文本".into(),
            });
        };
        log::info!(
            "web question extracted: len={} preview={:?}",
            question.len(),
            question.chars().take(200).collect::<String>(),
        );

        log::info!(
            "web ask: platform={} model={} stream={} has_tools={} tools_count={} has_role_tool={} dsml_always_parse={}",
            platform,
            request.model,
            request.stream,
            request.messages.get("tools").is_some(),
            request.messages.get("tools").and_then(|t| t.as_array()).map(|a| a.len()).unwrap_or(0),
            ai_web_provider::dsml::return_trip::has_tool_messages(&request.messages),
            std::env::var("DSML_ALWAYS_PARSE").unwrap_or_default(),
        );


        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if !request.stream {
            // 回程分支：含 role=tool 消息时走回程路径
            if ai_web_provider::dsml::return_trip::has_tool_messages(&request.messages) {
                return handle_web_return_trip(bridge.as_ref(), &platform, request, created).await;
            }

            let reply = bridge
                .ask(&platform, question.clone())
                .await
                .map_err(AppError::Provider)?;
            let usage_in = sse::estimate_tokens_from_text(&question);
            let usage_out =
                sse::estimate_tokens_from_text(&reply.reply) + sse::estimate_tokens_from_text(&reply.thinking);

            // DSML 解析：有 tools 白名单时启用
            let tools_whitelist = extract_tools_whitelist_with_fallback(&request.messages);
            let body = if let Some(whitelist) = &tools_whitelist {
                let config = dsml_config();
                let metrics = dsml_metrics();
                let whitelist_refs: Vec<&str> = whitelist.iter().map(|s| s.as_str()).collect();
                let full_id = web_completion_id();
                let id_suffix = full_id.strip_prefix("chatcmpl-").unwrap_or(&full_id);
                let mut response = ai_web_provider::dsml::dsml_to_openai_completion(
                    &reply.reply,
                    &reply.thinking,
                    Some(&whitelist_refs),
                    &request.model,
                    config,
                    metrics,
                    id_suffix,
                    id_suffix,
                    created,
                );
                response["usage"] = json!({"prompt_tokens": usage_in, "completion_tokens": usage_out, "total_tokens": usage_in + usage_out});
                register_web_tool_calls(&response, id_suffix, dsml_registry());
                fill_required_params(&mut response, request.messages.get("tools"));
                response
            } else {
                let mut message = json!({"role": "assistant", "content": reply.reply});
                if !reply.thinking.is_empty() {
                    message["reasoning_content"] = json!(reply.thinking);
                }
                json!({
                    "id": web_completion_id(),
                    "object": "chat.completion",
                    "created": created,
                    "model": request.model,
                    "choices": [{
                        "index": 0,
                        "message": message,
                        "finish_reason": "stop"
                    }],
                    "usage": {"prompt_tokens": usage_in, "completion_tokens": usage_out, "total_tokens": usage_in + usage_out}
                })
            };
            return Ok(ExecutorOutput::Json(body));
        }

        // 输入 token 按实际发送文本估算（system 前缀 + 最后一条 user）。
        // 不依赖 engine 的兜底估算——那个按完整请求体算，会把未发送的
        // 历史 messages 也计入，导致 web 会话输入 token 虚高。
        let usage_in = sse::estimate_tokens_from_text(&question);
        let handle = bridge
            .ask_stream(&platform, question)
            .await
            .map_err(AppError::Provider)?;

        let (mut tx, rx) = futures::channel::mpsc::channel::<std::result::Result<actix_web::web::Bytes, std::io::Error>>(64);
        let model = request.model.clone();
        let id = web_completion_id();
        let tools_whitelist = extract_tools_whitelist_with_fallback(&request.messages);
        let dsml_config = dsml_config().clone();
        let dsml_metrics = dsml_metrics().clone();
        let tools_def = request.messages.get("tools").cloned();
        tokio::spawn(async move {
            // 首 chunk：角色声明
            let _ = tx
                .send(Ok(web_chunk_frame(&id, created, &model, json!({"role": "assistant"}), None)))
                .await;

            let mut dsml_parser = tools_whitelist.map(|w| {
                ai_web_provider::dsml::stream::DsmlStreamParser::new(
                    dsml_config.clone(),
                    Some(w),
                    dsml_metrics.clone(),
                    id.clone(),
                )
            });
            let mut has_tool_calls = false;
            let mut last_tool_name = String::new();

            // 空闲心跳：SSE 注释行，防长思考期间代理/客户端超时断连
            let mut ping = tokio::time::interval(Duration::from_secs(15));
            ping.tick().await;

            let mut out_text = String::new();
            let mut chunks = handle.chunks;
            loop {
                let chunk = tokio::select! {
                    c = chunks.next() => match c {
                        Some(c) => c,
                        None => break,
                    },
                    _ = ping.tick() => {
                        let _ = tx.send(Ok(actix_web::web::Bytes::from(": ping\n\n"))).await;
                        continue;
                    }
                };
                if chunk.text.is_empty() {
                    continue;
                }
                match chunk.kind {
                    WebChunkKind::Thinking => {
                        let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({"reasoning_content": chunk.text}), None))).await;
                    }
                    WebChunkKind::Text => {
                        out_text.push_str(&chunk.text);
                        if let Some(ref mut parser) = dsml_parser {
                            let events = parser.feed(&chunk.text);
                            for event in events {
                                match event {
                                    ai_web_provider::dsml::DeltaEvent::Content(t) if !t.is_empty() => {
                                        let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({"content": t}), None))).await;
                                    }
                                    ai_web_provider::dsml::DeltaEvent::ToolCallStart { index, id: tc_id, name } => {
                                        has_tool_calls = true;
                                        last_tool_name = name.clone();
                                        let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({
                                            "tool_calls": [{"index": index, "id": tc_id, "type": "function", "function": {"name": name}}]
                                        }), None))).await;
                                    }
                                    ai_web_provider::dsml::DeltaEvent::ToolCallArguments { index, arguments_chunk } => {
                                        let filled = fill_arguments_chunk(&arguments_chunk, &last_tool_name, tools_def.as_ref());
                                        let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({
                                            "tool_calls": [{"index": index, "function": {"arguments": filled}}]
                                        }), None))).await;
                                    }
                                    ai_web_provider::dsml::DeltaEvent::Degrade { reason, snippet } => {
                                        ai_web_provider::dsml::degrade::handle(&id, reason, &snippet, &dsml_config, &dsml_metrics);
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({"content": chunk.text}), None))).await;
                        }
                    }
                    WebChunkKind::Status => {
                        let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({"vortex_status": chunk.text}), None))).await;
                    }
                }
            }

            // DsmlStreamParser 收尾
            if let Some(ref mut parser) = dsml_parser {
                let events = parser.finalize();
                for event in events {
                    if let ai_web_provider::dsml::DeltaEvent::Content(t) = event {
                        let _ = tx.send(Ok(web_chunk_frame(&id, created, &model, json!({"content": t}), None))).await;
                    }
                }
            }

            // 终态：成功发 stop/tool_calls 帧，失败发错误帧
            let final_ok = match handle.final_reply.await {
                Ok(_) => {
                    let finish = if has_tool_calls { "tool_calls" } else { "stop" };
                    let _ = tx
                        .send(Ok(web_chunk_frame(&id, created, &model, json!({}), Some(finish))))
                        .await;
                    true
                }
                Err(e) => {
                    let err = json!({"error": {"message": e, "type": "api_error"}});
                    let _ = tx
                        .send(Ok(actix_web::web::Bytes::from(format!("data: {err}\n\n"))))
                        .await;
                    false
                }
            };
            let _ = tx
                .send(Ok(actix_web::web::Bytes::from("data: [DONE]\n\n")))
                .await;

            // 流结束：用量按实际收发估算
            if let Some(cb) = usage_cb {
                cb(
                    StreamUsage {
                        input_tokens: usage_in,
                        output_tokens: sse::estimate_tokens_from_text(&out_text),
                        cache_creation: 0,
                        cache_read: 0,
                        reasoning: 0,
                        estimated: true,
                    },
                    final_ok,
                );
            }
        });

        Ok(ExecutorOutput::Stream(Box::pin(rx)))
    }
}

/// Web 回程处理：DSH 发回 role=tool → 合成自然语言文本 → bridge.ask → 解析新一轮 DSML。
async fn handle_web_return_trip(
    bridge: &dyn WebChatBridge,
    platform: &str,
    request: &ProxyRequest,
    created: u64,
) -> Result<ExecutorOutput> {
    let tool_messages =
        ai_web_provider::dsml::return_trip::extract_tool_messages(&request.messages);
    if tool_messages.is_empty() {
        return Ok(ExecutorOutput::UpstreamError {
            status: 400,
            body: "role=tool 消息无 content".into(),
        });
    }

    let registry = dsml_registry();
    let config = dsml_config();
    let metrics = dsml_metrics();

    let mut parts = Vec::new();
    for (call_id, content) in &tool_messages {
        match registry.lookup_by_call_id(call_id) {
            Some(invoke) => {
                parts.push(format!("工具 {} 的执行结果：\n{}", invoke.name, content));
            }
            None => {
                metrics.record_registry_miss();
                parts.push(content.clone());
            }
        }
    }
    let mut return_text = parts.join("\n\n");

    // 追加最后一条 role=user 消息（用户的新问题），否则 deepseek 只收到工具结果看不到问题
    if let Some(msgs) = request.messages.get("messages").and_then(|m| m.as_array()) {
        if let Some(last) = msgs.last() {
            if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                if let Some(content) = last.get("content").and_then(|c| c.as_str()) {
                    if !content.is_empty() {
                        return_text = format!("{return_text}\n\n{content}");
                    }
                }
            }
        }
    }

    log::info!(
        "web return_trip: tool_count={} return_text_len={} return_text_preview={:?}",
        tool_messages.len(),
        return_text.len(),
        return_text.chars().take(200).collect::<String>(),
    );
    let _ = std::fs::write(
        "/tmp/vortex_debug.log",
        format!("[return_trip] tool_count={} return_text_len={} preview={:?}\n", tool_messages.len(), return_text.len(), return_text.chars().take(500).collect::<String>()),
    );

    let reply = bridge
        .ask(platform, return_text.clone())
        .await
        .map_err(AppError::Provider)?;

    let usage_in = sse::estimate_tokens_from_text(&return_text);
    let usage_out = sse::estimate_tokens_from_text(&reply.reply)
        + sse::estimate_tokens_from_text(&reply.thinking);

    let tools_whitelist = extract_tools_whitelist_with_fallback(&request.messages);
    let full_id = web_completion_id();
    let id_suffix = full_id.strip_prefix("chatcmpl-").unwrap_or(&full_id);
    let whitelist_refs: Vec<&str> = tools_whitelist
        .as_ref()
        .map(|w| w.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    let mut response = ai_web_provider::dsml::dsml_to_openai_completion(
        &reply.reply,
        &reply.thinking,
        if tools_whitelist.is_some() {
            Some(&whitelist_refs)
        } else {
            None
        },
        &request.model,
        config,
        metrics,
        id_suffix,
        id_suffix,
        created,
    );
    response["usage"] = json!({"prompt_tokens": usage_in, "completion_tokens": usage_out, "total_tokens": usage_in + usage_out});
    register_web_tool_calls(&response, id_suffix, registry);
    fill_required_params(&mut response, request.messages.get("tools"));

    Ok(ExecutorOutput::Json(response))
}

#[cfg(test)]
mod web_tests {
    use super::*;

    #[test]
    fn web_question_extracts_system_and_last_user() {
        let body = json!({
            "model": "web-doubao",
            "messages": [
                {"role": "system", "content": "你是助手"},
                {"role": "user", "content": "第一问"},
                {"role": "assistant", "content": "第一答"},
                {"role": "user", "content": [{"type": "text", "text": "第二问"}]},
            ]
        });
        let q = web_question(&body).unwrap();
        assert!(q.starts_with("你是助手"));
        assert!(q.ends_with("第二问"));
        assert!(q.contains("\n\n"));
        // 历史 user 消息不回放
        assert!(!q.contains("第一问"));
    }

    #[test]
    fn web_question_empty_body() {
        assert!(web_question(&json!({"messages": []})).is_none());
        assert!(web_question(&json!({})).is_none());
        // 只有 assistant 历史
        let body = json!({"messages": [{"role": "assistant", "content": "答"}]});
        assert!(web_question(&body).is_none());
    }

    #[test]
    fn web_chunk_frame_shape() {
        let frame = web_chunk_frame("id1", 123, "web-doubao", json!({"content": "hi"}), None);
        let s = String::from_utf8(frame.to_vec()).unwrap();
        assert!(s.starts_with("data: {"));
        assert!(s.ends_with("\n\n"));
        assert!(s.contains("\"object\":\"chat.completion.chunk\""));
        let frame2 = web_chunk_frame("id1", 123, "web-doubao", json!({}), Some("stop"));
        assert!(String::from_utf8(frame2.to_vec()).unwrap().contains("\"finish_reason\":\"stop\""));
    }
}
