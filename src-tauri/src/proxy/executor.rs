use crate::db::models::ProxyRequest;
use crate::db::models::ProviderConnection;
use crate::providers::types::ProviderDef;
use crate::AppState;
use crate::error::{AppError, Result};
use serde_json::json;
use std::sync::Arc;

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
    ) -> Result<actix_web::HttpResponse>;
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
    ) -> Result<actix_web::HttpResponse> {
        let client = &state.http_client;
        let base_url = connection.provider_specific_data.get("baseUrl")
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
        if let Some(ref api_key) = connection.api_key {
            req_builder = req_builder.bearer_auth(api_key);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("Request failed: {}", e)))?;

        let status = response.status().as_u16();
        if status >= 400 {
            let error_body = response.text().await.unwrap_or_default();
            return Err(AppError::Provider(format!("Provider returned {}: {}", status, error_body)));
        }

        if request.stream {
            Ok(crate::proxy::sse::stream_sse_response(response, "openai").await)
        } else {
            let json_body = response.json::<serde_json::Value>().await
                .map_err(|e| AppError::Provider(format!("Failed to parse response: {}", e)))?;
            Ok(actix_web::HttpResponse::Ok().json(json_body))
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
    ) -> Result<actix_web::HttpResponse> {
        let client = &state.http_client;
        let base_url = connection.provider_specific_data.get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or(&def.base_url);
        let url = format!("{}{}", base_url, def.chat_path);

        let mut system_msg: Option<serde_json::Value> = None;
        let mut messages = vec![];

        if let Some(obj) = request.messages.as_object() {
            if let Some(msgs) = obj.get("messages").and_then(|m| m.as_array()) {
                for msg in msgs {
                    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    let content = msg.get("content");

                    if role == "system" {
                        system_msg = content.cloned();
                    } else {
                        messages.push(msg.clone());
                    }
                }
            }
        }

        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "stream": request.stream,
        });

        if let Some(sys) = system_msg {
            body.as_object_mut().unwrap().insert("system".to_string(), sys);
        }

        let api_key = connection.api_key.as_deref().ok_or_else(|| AppError::Unauthorized("API key required".into()))?;

        let response = client.post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("Anthropic request failed: {}", e)))?;

        let status = response.status().as_u16();
        if status >= 400 {
            let error_body = response.text().await.unwrap_or_default();
            return Err(AppError::Provider(format!("Anthropic returned {}: {}", status, error_body)));
        }

        if request.stream {
            Ok(crate::proxy::sse::stream_sse_response(response, "anthropic").await)
        } else {
            let json_body = response.json::<serde_json::Value>().await
                .map_err(|e| AppError::Provider(format!("Failed to parse Anthropic response: {}", e)))?;
            let openai_response = crate::translator::anthropic_to_openai_response(&json_body, model);
            Ok(actix_web::HttpResponse::Ok().json(openai_response))
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
    ) -> Result<actix_web::HttpResponse> {
        let client = &state.http_client;
        let api_key = connection.api_key.as_deref().ok_or_else(|| AppError::Unauthorized("API key required".into()))?;

        let chat_path = if request.stream {
            def.chat_path.replace("{model}", model).replace("generateContent", "streamGenerateContent")
        } else {
            def.chat_path.replace("{model}", model)
        };
        let url = format!("{}{}?key={}", def.base_url, chat_path, api_key);

        let mut system_instruction: Option<serde_json::Value> = None;
        let mut contents = vec![];

        if let Some(obj) = request.messages.as_object() {
            if let Some(msgs) = obj.get("messages").and_then(|m| m.as_array()) {
                for msg in msgs {
                    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    let content_str = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

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

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(1.0),
                "maxOutputTokens": request.max_tokens.unwrap_or(8192),
            }
        });

        if let Some(sys) = system_instruction {
            body.as_object_mut().unwrap().insert("systemInstruction".to_string(), sys);
        }

        let response = client.post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("Gemini request failed: {}", e)))?;

        let status = response.status().as_u16();
        if status >= 400 {
            let error_body = response.text().await.unwrap_or_default();
            return Err(AppError::Provider(format!("Gemini returned {}: {}", status, error_body)));
        }

        if request.stream {
            Ok(crate::proxy::sse::stream_sse_response(response, "gemini").await)
        } else {
            let json_body = response.json::<serde_json::Value>().await
                .map_err(|e| AppError::Provider(format!("Failed to parse Gemini response: {}", e)))?;
            let openai_response = crate::translator::gemini_to_openai_response(&json_body, model);
            Ok(actix_web::HttpResponse::Ok().json(openai_response))
        }
    }
}
