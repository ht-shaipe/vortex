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

pub struct ExecutorFactory;

impl ExecutorFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn create(&self, def: &ProviderDef) -> Box<dyn ProviderExecutor + Send + Sync> {
        match def.api_format.as_str() {
            "anthropic" => Box::new(AnthropicExecutor),
            "gemini" => Box::new(GeminiExecutor),
            _ => Box::new(OpenAIExecutor),
        }
    }
}

#[async_trait::async_trait]
pub trait ProviderExecutor: Send + Sync {
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

pub struct OpenAIExecutor;

#[async_trait::async_trait]
impl ProviderExecutor for OpenAIExecutor {
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

        let url = format!("{}{}", base_url, def.chat_path);

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

        if response.status().as_u16() >= 400 {
            return Ok(upstream_error(response).await);
        }

        if request.stream {
            Ok(ExecutorOutput::Stream(
                sse::stream_sse_response(response, "openai").await,
            ))
        } else {
            let json_body = response
                .json::<Value>()
                .await
                .map_err(|e| AppError::Provider(format!("Failed to parse response: {}", e)))?;
            Ok(ExecutorOutput::Json(json_body))
        }
    }
}

pub struct AnthropicExecutor;

#[async_trait::async_trait]
impl ProviderExecutor for AnthropicExecutor {
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
        let url = format!("{}{}", base_url, def.chat_path);

        // OpenAI 消息 → Anthropic 消息（含 tool_calls → tool_use 全链路转换）
        let (system, messages) = crate::translator::openai_messages_to_anthropic(&request.messages);

        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "stream": request.stream,
        });

        if let Some(sys) = system {
            body["system"] = sys;
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(t) = request.top_p {
            body["top_p"] = json!(t);
        }
        if let Some(stops) = request.messages.get("stop").and_then(|s| s.as_array()) {
            body["stop_sequences"] = json!(stops);
        }
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
            Ok(ExecutorOutput::Stream(
                sse::stream_sse_response(response, "anthropic").await,
            ))
        } else {
            let json_body = response
                .json::<Value>()
                .await
                .map_err(|e| AppError::Provider(format!("Failed to parse Anthropic response: {}", e)))?;
            let openai_response = crate::translator::anthropic_to_openai_response(&json_body, model);
            Ok(ExecutorOutput::Json(openai_response))
        }
    }
}

pub struct GeminiExecutor;

#[async_trait::async_trait]
impl ProviderExecutor for GeminiExecutor {
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

        let chat_path = if request.stream {
            def.chat_path
                .replace("{model}", model)
                .replace("generateContent", "streamGenerateContent")
        } else {
            def.chat_path.replace("{model}", model)
        };
        let url = format!("{}{}?key={}", def.base_url, chat_path, api_key);

        let mut system_instruction: Option<Value> = None;
        let mut contents = vec![];

        if let Some(obj) = request.messages.as_object() {
            if let Some(msgs) = obj.get("messages").and_then(|m| m.as_array()) {
                for msg in msgs {
                    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    let content_str = crate::translator::extract_text_content(msg.get("content"));

                    if role == "system" {
                        system_instruction = Some(json!({"parts": [{"text": content_str}]}));
                    } else {
                        let gemini_role = if role == "assistant" { "model" } else { "user" };
                        contents.push(json!({
                            "role": gemini_role,
                            "parts": [{"text": content_str}]
                        }));
                    }
                }
            }
        }

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
            Ok(ExecutorOutput::Stream(
                sse::stream_sse_response(response, "gemini").await,
            ))
        } else {
            let json_body = response
                .json::<Value>()
                .await
                .map_err(|e| AppError::Provider(format!("Failed to parse Gemini response: {}", e)))?;
            let openai_response = crate::translator::gemini_to_openai_response(&json_body, model);
            Ok(ExecutorOutput::Json(openai_response))
        }
    }
}
