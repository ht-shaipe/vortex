//! Playground 端点：直接调用代理引擎测试请求。

use actix_web::{web, HttpResponse};
use vortex_store::db::models::ProxyRequest;
use crate::AppState;
use serde_json::json;

/// Playground 调试请求体（camelCase，与前端保持一致）。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundRequest {
    /// 待测试的模型名（可带提供商前缀）
    pub model: String,
    /// OpenAI 格式消息数组或完整请求体
    pub messages: serde_json::Value,
    /// 是否以 SSE 流式返回，默认 false
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<i64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    /// 透传给上游的额外参数
    #[serde(default)]
    pub extra: Option<serde_json::Value>,
}

/// 处理 `POST /api/playground`，走完整代理链路执行一次测试请求。
///
/// 与真实客户端请求的区别：不带 API Key、不标记 agent（不参与智能体避让），
/// 其余鉴权/路由/重试/计费流程与 `/v1/chat/completions` 一致。
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
        agent: None, // 管理端调试请求，不参与避让
        source_format: "openai".to_string(),
        extra: req.extra.unwrap_or(json!({})),
        saved_tokens: 0,
    };

    // 引擎无内部可变性，直接借用处理请求（避免锁守卫跨 await）
    let upstream_client = crate::create_awc_client();
    let app_state: &AppState = &*state;
    match app_state
        .proxy_engine
        .handle_request(app_state, &upstream_client, request)
        .await
    {
        Ok(vortex_router::proxy::engine::ProxyOutput::Response(body)) => HttpResponse::Ok().json(body),
        Ok(vortex_router::proxy::engine::ProxyOutput::Stream(stream)) => {
            vortex_router::proxy::sse::sse_http_response(stream)
        }
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": format!("{}", e) })),
    }
}
