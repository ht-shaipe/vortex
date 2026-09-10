//! 上游执行器
//!
//! 负责不同 API 格式的请求构建和发送。多级超时保护（借鉴 relay-rs）：
//!
//! - 连接超时 10s（HTTP 客户端全局配置）
//! - 流式首字节 60s（发出请求到收到响应头，`tokio::time::timeout` 包裹）
//! - 非流式整体 300s（reqwest 请求级超时，含读取响应体）
//! - 流式逐块 120s（在 sse.rs 中按次计时，不限制总时长）
//!
//! 请求不会因全局总超时被中途杀掉，长流式生成不受影响。

use crate::db::models::{ProxyRequest, ProviderConnection};
use crate::providers::types::ProviderDef;
use crate::AppState;
use crate::error::{AppError, Result};
use crate::proxy::sse::{self, SseStream};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

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
    ///
    /// # 返回
    /// 按格式匹配的执行器：`anthropic` → AnthropicExecutor，
    /// `gemini` → GeminiExecutor，其他 → OpenAIExecutor
    pub fn create(&self, def: &ProviderDef) -> Box<dyn ProviderExecutor + Send + Sync> {
        match def.api_format.as_str() {
            "anthropic" => Box::new(AnthropicExecutor),
            "gemini" => Box::new(GeminiExecutor),
            _ => Box::new(OpenAIExecutor),
        }
    }
}

/// 提供商执行器 trait
///
/// 定义向上游 AI 提供商发送请求的统一接口，
/// 不同 API 格式的执行器各自实现此 trait。
#[async_trait::async_trait]
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
        state: &Arc<AppState>,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
    ) -> Result<ExecutorOutput>;
}

/// 发送请求并应用首字节超时（防止上游迟迟不返回响应头）
async fn send_with_first_byte_timeout(
    builder: reqwest::RequestBuilder,
) -> std::result::Result<reqwest::Response, AppError> {
    tokio::time::timeout(FIRST_BYTE_TIMEOUT, builder.send())
        .await
        .map_err(|_| {
            AppError::Provider(format!(
                "上游响应超时（{} 秒未收到响应头）",
                FIRST_BYTE_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| AppError::Provider(format!("Request failed: {}", e)))
}

/// 读取上游错误响应体，构造 UpstreamError
async fn upstream_error(response: reqwest::Response) -> ExecutorOutput {
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    ExecutorOutput::UpstreamError { status, body }
}

/// 拼接上游 URL 并做版本段归一化。
///
/// 连接级 baseUrl 通常已含版本前缀（如 `https://host/v1`），而提供商定义的
/// `chat_path` 也以 `/v1` 开头，直接相加会得到 `/v1/v1/chat/completions`（上游 404）。
/// 此处：当 baseUrl 以 `/v1` 结尾且 path 以 `v1/` 开头时去掉重复段，其余情况原样拼接。
fn build_upstream_url(base_url: &str, chat_path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let path = chat_path.strip_prefix('/').unwrap_or(chat_path);
    let path = if base.ends_with("/v1") && path.starts_with("v1/") {
        &path["v1/".len()..]
    } else {
        path
    };
    format!("{}/{}", base, path)
}

/// OpenAI 格式执行器
///
/// 直接使用 OpenAI chat/completions 接口格式，请求与响应无需转换。
pub struct OpenAIExecutor;

#[async_trait::async_trait]
impl ProviderExecutor for OpenAIExecutor {
    /// 执行 OpenAI 格式请求
    ///
    /// 构建请求体（注入 model、temperature、max_tokens 等参数），
    /// 设置 Bearer 认证，发送请求并处理流式/非流式响应。
    async fn execute(
        &self,
        state: &Arc<AppState>,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
    ) -> Result<ExecutorOutput> {
        let client = &state.http_client;
        // 优先使用连接级 baseUrl，回退到提供商定义的默认值
        let base_url = connection
            .provider_specific_data
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or(&def.base_url);

        let url = build_upstream_url(base_url, &def.chat_path);

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
        }

        let mut req_builder = client.post(&url);
        // 非流式请求应用整体超时；流式只管首字节，逐块超时由 sse.rs 处理
        if !request.stream {
            req_builder = req_builder.timeout(REQUEST_TIMEOUT);
        }
        if let Some(ref api_key) = connection.api_key {
            req_builder = req_builder.bearer_auth(api_key);
        }

        let response = send_with_first_byte_timeout(req_builder.json(&body)).await?;

        // 上游返回错误
        if response.status().as_u16() >= 400 {
            return Ok(upstream_error(response).await);
        }

        if request.stream {
            // 流式：返回归一化 SSE 流
            Ok(ExecutorOutput::Stream(
                sse::stream_sse_response(response, "openai").await,
            ))
        } else {
            // 非流式：解析 JSON 响应
            let json_body = response
                .json::<Value>()
                .await
                .map_err(|e| AppError::Provider(format!("Failed to parse response: {}", e)))?;
            Ok(ExecutorOutput::Json(json_body))
        }
    }
}

/// Anthropic 格式执行器
///
/// 将 OpenAI 格式请求转换为 Anthropic Messages 格式发送，
/// 并将非流式响应转回 OpenAI 格式。
pub struct AnthropicExecutor;

#[async_trait::async_trait]
impl ProviderExecutor for AnthropicExecutor {
    /// 执行 Anthropic 格式请求
    ///
    /// 将 OpenAI 消息/工具定义转换为 Anthropic 格式，设置 x-api-key 认证，
    /// 发送请求。非流式响应通过转换器转回 OpenAI 格式。
    async fn execute(
        &self,
        state: &Arc<AppState>,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
    ) -> Result<ExecutorOutput> {
        let client = &state.http_client;
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
            .and_then(|t| crate::translator::openai_tools_to_anthropic(t))
        {
            body["tools"] = tools;
            if let Some(tc) = request
                .messages
                .get("tool_choice")
                .and_then(|t| crate::translator::openai_tool_choice_to_anthropic(t))
            {
                body["tool_choice"] = tc;
            }
        }

        let api_key = connection
            .api_key
            .as_deref()
            .ok_or_else(|| AppError::Unauthorized("API key required".into()))?;

        // Anthropic 认证：x-api-key 头 + 版本头
        let mut req_builder = client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        if !request.stream {
            req_builder = req_builder.timeout(REQUEST_TIMEOUT);
        }

        let response = send_with_first_byte_timeout(req_builder.json(&body)).await?;

        if response.status().as_u16() >= 400 {
            return Ok(upstream_error(response).await);
        }

        if request.stream {
            // 流式：Anthropic SSE → OpenAI chunk 格式
            Ok(ExecutorOutput::Stream(
                sse::stream_sse_response(response, "anthropic").await,
            ))
        } else {
            // 非流式：解析 Anthropic 响应并转回 OpenAI 格式
            let json_body = response
                .json::<Value>()
                .await
                .map_err(|e| AppError::Provider(format!("Failed to parse Anthropic response: {}", e)))?;
            let openai_response = crate::translator::anthropic_to_openai_response(&json_body, model);
            Ok(ExecutorOutput::Json(openai_response))
        }
    }
}

/// Gemini 格式执行器
///
/// 将 OpenAI 格式请求转换为 Gemini generateContent 格式发送，
/// 并将非流式响应转回 OpenAI 格式。API key 通过 URL 查询参数传递。
pub struct GeminiExecutor;

#[async_trait::async_trait]
impl ProviderExecutor for GeminiExecutor {
    /// 执行 Gemini 格式请求
    ///
    /// 将 OpenAI 消息转换为 Gemini contents 格式（system → systemInstruction，
    /// assistant → model 角色），设置 generationConfig，通过 URL 参数传递 API key。
    async fn execute(
        &self,
        state: &Arc<AppState>,
        request: &ProxyRequest,
        def: &ProviderDef,
        connection: &ProviderConnection,
        model: &str,
    ) -> Result<ExecutorOutput> {
        let client = &state.http_client;
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
        if let Some(obj) = request.messages.as_object() {
            if let Some(msgs) = obj.get("messages").and_then(|m| m.as_array()) {
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

        let mut req_builder = client.post(&url).header("content-type", "application/json");
        if !request.stream {
            req_builder = req_builder.timeout(REQUEST_TIMEOUT);
        }

        let response = send_with_first_byte_timeout(req_builder.json(&body)).await?;

        if response.status().as_u16() >= 400 {
            return Ok(upstream_error(response).await);
        }

        if request.stream {
            // 流式：Gemini SSE → OpenAI chunk 格式
            Ok(ExecutorOutput::Stream(
                sse::stream_sse_response(response, "gemini").await,
            ))
        } else {
            // 非流式：解析 Gemini 响应并转回 OpenAI 格式
            let json_body = response
                .json::<Value>()
                .await
                .map_err(|e| AppError::Provider(format!("Failed to parse Gemini response: {}", e)))?;
            let openai_response = crate::translator::gemini_to_openai_response(&json_body, model);
            Ok(ExecutorOutput::Json(openai_response))
        }
    }
}
