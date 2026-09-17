//! Playground 端点：直接调用代理引擎测试请求。

use actix_web::{web, HttpResponse};
use crate::db::models::ProxyRequest;
use crate::AppState;
use serde_json::json;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundRequest {
    pub model: String,
    pub messages: serde_json::Value,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<i64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub extra: Option<serde_json::Value>,
}

pub async fn playground(
    state: web::Data<std::sync::Arc<AppState>>,
    body: web::Json<PlaygroundRequest>,
) -> HttpResponse {
    let req = body.into_inner();

    let request = ProxyRequest {
        model: req.model,
        messages: req.messages,
        stream: req.stream,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        top_p: req.top_p,
        api_key: None,
        source_format: "openai".to_string(),
        extra: req.extra.unwrap_or(json!({})),
    };

    let engine = state.proxy_engine.read();
    let upstream_client = crate::create_awc_client();
    match engine.handle_request(&state, &upstream_client, request).await {
        Ok(crate::proxy::engine::ProxyOutput::Response(body)) => HttpResponse::Ok().json(body),
        Ok(crate::proxy::engine::ProxyOutput::Stream(stream)) => {
            crate::proxy::sse::sse_http_response(stream)
        }
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": format!("{}", e) })),
    }
}
