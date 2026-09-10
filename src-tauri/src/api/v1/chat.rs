use actix_web::{web, HttpRequest, HttpResponse};
use crate::db::models::ProxyRequest;
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

pub async fn chat_completions(
    state: web::Data<Arc<AppState>>,
    body: web::Json<serde_json::Value>,
    req: HttpRequest,
) -> HttpResponse {
    let body_inner = body.into_inner();
    let request = match parse_chat_request(&body_inner, &req) {
        Ok(r) => r,
        Err(e) => return HttpResponse::BadRequest().json(json!({"error": {"message": e.to_string(), "type": "invalid_request_error"}})),
    };

    let engine = state.proxy_engine.read();
    match engine.handle_request(&state, request).await {
        Ok(response) => response,
        Err(e) => {
            let status = match &e {
                crate::error::AppError::Unauthorized(_) => actix_web::http::StatusCode::UNAUTHORIZED,
                crate::error::AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
                crate::error::AppError::Provider(_) => actix_web::http::StatusCode::BAD_GATEWAY,
                crate::error::AppError::Routing(_) => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            HttpResponse::build(status).json(json!({
                "error": {
                    "message": e.to_string(),
                    "type": format!("{:?}", status),
                    "code": status.as_u16(),
                }
            }))
        }
    }
}

fn parse_chat_request(body: &serde_json::Value, req: &HttpRequest) -> crate::error::Result<ProxyRequest> {
    let model = body.get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("model is required".into()))?
        .to_string();

    let stream = body.get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let api_key = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    Ok(ProxyRequest {
        model,
        messages: body.clone(),
        stream,
        temperature: body.get("temperature").and_then(|v| v.as_f64()),
        max_tokens: body.get("max_tokens").and_then(|v| v.as_i64()),
        top_p: body.get("top_p").and_then(|v| v.as_f64()),
        api_key,
        source_format: "openai".to_string(),
        extra: json!({}),
    })
}
