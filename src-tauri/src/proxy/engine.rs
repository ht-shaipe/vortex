use crate::db::models::{ProxyRequest, ResolvedComboTarget, RoutingContext, UsageEntry};
use crate::db::{core as db_core, providers as db_providers, usage as db_usage, combos as db_combos};
use crate::error::{AppError, Result};
use crate::providers::ProviderRegistry;
use crate::proxy::executor::ExecutorFactory;
use crate::proxy::retry::RetryPolicy;
use crate::routing::RoutingEngine;
use crate::AppState;
use std::sync::Arc;
use std::time::Instant;

pub struct ProxyEngine {
    executor_factory: ExecutorFactory,
    retry_policy: RetryPolicy,
}

impl ProxyEngine {
    pub fn new() -> Self {
        Self {
            executor_factory: ExecutorFactory::new(),
            retry_policy: RetryPolicy::new(2, 500),
        }
    }

    pub async fn handle_request(
        &self,
        state: &Arc<AppState>,
        request: ProxyRequest,
    ) -> Result<actix_web::HttpResponse> {
        let start = Instant::now();
        let conn = db_core::get_conn(&state.db_pool)?;

        if state.config.require_api_key {
            if let Some(ref api_key) = request.api_key {
                let key_record = db_api_keys::get_by_key(&conn, api_key)?;
                if key_record.is_none() {
                    return Err(AppError::Unauthorized("Invalid API key".into()));
                }
                let key_obj = key_record.unwrap();
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

        let (provider_def, connection, model, resolved_targets) =
            self.resolve_route(&conn, &state.provider_registry, &state.routing_engine, &request, &state.encryption_key)?;

        let result = if let Some(targets) = resolved_targets {
            self.handle_combo_request(state, &conn, &request, &targets, start).await
        } else {
            let def = provider_def.ok_or_else(|| AppError::Provider("No provider definition".into()))?;
            let conn_obj = connection.ok_or_else(|| AppError::Provider("No active connection".into()))?;
            self.handle_single_with_retry(state, &conn, &request, &def, &conn_obj, &model, start).await
        };

        match &result {
            Ok(_) => {}
            Err(e) => {
                let latency = start.elapsed().as_millis() as i64;
                let entry = UsageEntry {
                    id: 0,
                    provider: None,
                    model: Some(request.model.clone()),
                    connection_id: None,
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
                    combo_strategy: None,
                    cost: 0.0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = db_usage::record(&conn, &entry);
            }
        }

        result
    }

    fn resolve_route(
        &self,
        conn: &rusqlite::Connection,
        registry: &ProviderRegistry,
        engine: &RoutingEngine,
        request: &ProxyRequest,
        enc_key: &[u8],
    ) -> Result<(Option<crate::providers::types::ProviderDef>, Option<crate::db::models::ProviderConnection>, String, Option<Vec<ResolvedComboTarget>>)> {
        let combo_result = db_combos::get_by_name(conn, &request.model)?;
        if let Some(combo) = combo_result {
            let context = RoutingContext::default();
            let targets = crate::routing::combo::resolve_combo(conn, &combo, &context, engine, enc_key)?;
            return Ok((None, None, request.model.clone(), Some(targets)));
        }

        if let Some((provider_id, def, model)) = registry.resolve_model_provider(&request.model) {
            let connections = db_providers::list_by_provider(conn, &provider_id, enc_key)?;
            let connection = connections.into_iter().next();
            return Ok((Some(def.clone()), connection, model, None));
        }

        Err(AppError::Routing(format!("Cannot resolve model: {}", request.model)))
    }

    async fn handle_single_with_retry(
        &self,
        state: &Arc<AppState>,
        conn: &rusqlite::Connection,
        request: &ProxyRequest,
        def: &crate::providers::types::ProviderDef,
        connection: &crate::db::models::ProviderConnection,
        model: &str,
        start: Instant,
    ) -> Result<actix_web::HttpResponse> {
        let executor = self.executor_factory.create(def);
        let mut last_err = None;

        for attempt in 0..=self.retry_policy.max_retries() {
            let breaker_name = format!("{}:{}", def.id, connection.id);
            if !state.resilience_manager.is_available(&breaker_name) {
                last_err = Some(AppError::Provider(format!("Circuit breaker open for {}", breaker_name)));
                continue;
            }

            match executor.execute(state, request, def, connection, model).await {
                Ok(response) => {
                    let status = response.status();
                    let body_bytes = actix_web::body::to_bytes(response.into_body()).await.unwrap_or_default();

                    if status.is_success() {
                        let (tokens_in, tokens_out, cost) = extract_usage_from_response(&body_bytes, def.api_format.as_str());
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
                            combo_strategy: None,
                            cost,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };
                        let _ = db_usage::record(conn, &entry);

                        return Ok(actix_web::HttpResponseBuilder::new(status).body(body_bytes.to_vec()));
                    }

                    let status_code = status.as_u16();
                    if !self.retry_policy.should_retry(attempt, Some(status_code)) {
                        if status_code == 429 {
                            let retry_after = chrono::Utc::now() + chrono::Duration::seconds(60);
                            let _ = db_providers::increment_backoff(conn, &connection.id, Some(retry_after.to_rfc3339().as_str()));
                        }
                        state.resilience_manager.record_failure(&breaker_name);
                        return Err(AppError::Provider(format!("Provider returned {}: {}", status_code, String::from_utf8_lossy(&body_bytes))));
                    }

                    let delay = self.retry_policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(AppError::Provider(format!("Provider returned {} (attempt {}), retrying", status_code, attempt + 1)));
                }
                Err(e) => {
                    let breaker_name = format!("{}:{}", def.id, connection.id);
                    state.resilience_manager.record_failure(&breaker_name);
                    if !self.retry_policy.should_retry(attempt, None) {
                        return Err(e);
                    }
                    let delay = self.retry_policy.delay_for_attempt(attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| AppError::Provider("All retries exhausted".into())))
    }

    async fn handle_combo_request(
        &self,
        state: &Arc<AppState>,
        conn: &rusqlite::Connection,
        request: &ProxyRequest,
        targets: &[ResolvedComboTarget],
        start: Instant,
    ) -> Result<actix_web::HttpResponse> {
        let mut last_error = None;

        for target in targets {
            let provider_def = state.provider_registry.get(&target.provider_id);
            if provider_def.is_none() {
                continue;
            }
            let def = provider_def.unwrap();

            let connection = if let Some(ref cid) = target.connection_id {
                db_providers::get_by_id(conn, cid, &state.encryption_key)?
            } else {
                db_providers::list_by_provider(conn, &target.provider_id, &state.encryption_key)?.into_iter().next()
            };

            if connection.is_none() {
                continue;
            }
            let conn_obj = connection.unwrap();

            match self.handle_single_with_retry(state, conn, request, &def, &conn_obj, &target.model_str, start).await {
                Ok(response) => {
                    let status = response.status();
                    let body_bytes = actix_web::body::to_bytes(response.into_body()).await.unwrap_or_default();
                    let (tokens_in, tokens_out, cost) = extract_usage_from_response(&body_bytes, def.api_format.as_str());
                    let latency = start.elapsed().as_millis() as i64;
                    let entry = UsageEntry {
                        id: 0,
                        provider: Some(def.id.clone()),
                        model: Some(target.model_str.clone()),
                        connection_id: Some(conn_obj.id.clone()),
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
                        combo_strategy: Some(target.step_id.clone()),
                        cost,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = db_usage::record(conn, &entry);
                    return Ok(actix_web::HttpResponseBuilder::new(status).body(body_bytes.to_vec()));
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AppError::Routing("All targets failed".into())))
    }
}

fn extract_usage_from_response(body: &[u8], api_format: &str) -> (i64, i64, f64) {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let parsed = match parsed {
        Some(p) => p,
        None => return (0, 0, 0.0),
    };

    let (tokens_in, tokens_out) = match api_format {
        "anthropic" => {
            let input = parsed.get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = parsed.get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output)
        }
        "gemini" => {
            let meta = parsed.get("usageMetadata");
            let input = meta
                .and_then(|u| u.get("promptTokenCount"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = meta
                .and_then(|u| u.get("candidatesTokenCount"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output)
        }
        _ => {
            let input = parsed.get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let output = parsed.get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            (input, output)
        }
    };

    (tokens_in, tokens_out, 0.0)
}

use crate::db::api_keys as db_api_keys;
