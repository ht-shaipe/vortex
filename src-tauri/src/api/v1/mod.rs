//! OpenAI 兼容 API v1 端点模块
//!
//! 该模块聚合了所有面向客户端 SDK 的代理端点（聊天补全、模型列表、嵌入、图像、
//! Anthropic Messages），并提供统一的错误响应格式化工具：
//! - [`openai_error_response`]：将引擎错误包装为 OpenAI 风格 JSON。
//! - [`anthropic_error_response`]：将引擎错误包装为 Anthropic 风格 JSON。

/// POST /v1/chat/completions 聊天补全端点
pub mod chat;
/// POST /v1/messages Anthropic 兼容消息端点
pub mod messages;
/// POST /v1/responses OpenAI Responses API 兼容端点
pub mod responses;
/// GET /v1/models 模型列表端点
pub mod models;
/// POST /v1/embeddings 文本嵌入端点
pub mod embeddings;
/// POST /v1/images/generations 图像生成端点
pub mod images;
/// MCP 服务器端点
pub mod mcp;

use actix_web::HttpResponse;
use crate::error::AppError;
use serde_json::json;

/// 熔断错误对下游的友好提示文案。
///
/// 内部熔断器名称（`provider:connection:model`）对终端客户端没有意义，
/// 只在应用日志与请求日志的 error_code 中保留，下游统一收到可读提示。
const CIRCUIT_OPEN_MESSAGE: &str = "上游暂时不可用（熔断冷却中，约 1 分钟后自动恢复探测），请稍后重试";

/// 将引擎错误映射为 (HTTP 状态码, OpenAI 错误类型, 错误码)。
///
/// 该函数是 [`openai_error_response`] 与 [`anthropic_error_response`] 的共享底层，
/// 负责把内部 [`AppError`] 枚举映射为 HTTP 层可用的状态码与错误分类字符串。
///
/// - `BadRequest` → 400 / `invalid_request_error`
/// - `Unauthorized` → 401 / `authentication_error`
/// - `Routing` → 404 / `invalid_request_error` + `model_not_found`
/// - `CircuitOpen` → 503 / `api_error`（服务暂不可用，客户端可稍后重试）
/// - `Provider` → 502 / `api_error`
/// - 其他 → 500 / `api_error`
fn error_kind(e: &AppError) -> (actix_web::http::StatusCode, &'static str, Option<&'static str>) {
    match e {
        // 请求参数错误
        AppError::BadRequest(_) => (
            actix_web::http::StatusCode::BAD_REQUEST,
            "invalid_request_error",
            None,
        ),
        // 鉴权失败
        AppError::Unauthorized(_) => (
            actix_web::http::StatusCode::UNAUTHORIZED,
            "authentication_error",
            None,
        ),
        // 路由/模型未找到
        AppError::Routing(_) => (
            actix_web::http::StatusCode::NOT_FOUND,
            "invalid_request_error",
            Some("model_not_found"),
        ),
        // 熔断器打开：服务暂不可用，语义上用 503 提示客户端稍后重试
        AppError::CircuitOpen(_) => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            "api_error",
            None,
        ),
        // 上游提供方错误
        AppError::Provider(_) => (
            actix_web::http::StatusCode::BAD_GATEWAY,
            "api_error",
            None,
        ),
        // 兜底：内部错误
        _ => (
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "api_error",
            None,
        ),
    }
}

/// 提取面向下游的错误文案。
///
/// 熔断错误返回友好提示（熔断器名称仅保留在应用日志中），
/// 其余错误使用原始描述。
///
/// - `e`：引擎返回的应用错误
/// - 返回值：面向下游客户端的错误文案
fn error_message(e: &AppError) -> String {
    match e {
        AppError::CircuitOpen(_) => CIRCUIT_OPEN_MESSAGE.to_string(),
        _ => e.to_string(),
    }
}

/// 上游错误透传响应。
///
/// [`AppError::Upstream`] 携带上游原始状态码与响应体，直接透传给下游客户端：
/// - 响应体为 JSON 对象：原样返回（上游本身就是 OpenAI/Anthropic 风格错误结构），
///   方便下游客户端与排查工具看到上游真实错误信息；
/// - 响应体非 JSON：包装为 OpenAI 风格错误结构，原始文本放入 message。
///
/// - `e`：上游错误
/// - 返回值：与上游状态码一致的 [`HttpResponse`]
fn upstream_passthrough_response(e: &AppError) -> HttpResponse {
    let AppError::Upstream { status, body } = e else {
        return HttpResponse::InternalServerError().finish();
    };
    let status_code = actix_web::http::StatusCode::from_u16(*status)
        .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY);
    // 尝试按 JSON 原样透传
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if v.is_object() {
            return HttpResponse::build(status_code)
                .content_type("application/json")
                .body(body.clone());
        }
    }
    // 非 JSON 响应体：包装为 OpenAI 风格错误
    HttpResponse::build(status_code).json(json!({
        "error": {
            "message": body,
            "type": "upstream_error",
            "code": status,
        }
    }))
}

/// OpenAI 风格错误响应
///
/// 将 [`AppError`] 转换为 OpenAI API 兼容的错误 JSON 结构：
/// ```json
/// { "error": { "message": "...", "type": "...", "code": "..." } }
/// ```
///
/// - `e`：引擎返回的应用错误
/// - 返回值：带对应 HTTP 状态码的 [`HttpResponse`]
pub fn openai_error_response(e: &AppError) -> HttpResponse {
    // 上游错误：透传上游状态码与原始响应体，方便下游排查
    if matches!(e, AppError::Upstream { .. }) {
        return upstream_passthrough_response(e);
    }
    let (status, error_type, code) = error_kind(e); // 解构状态码、类型、可选错误码
    let mut err = json!({
        "message": error_message(e),
        "type": error_type,
    });
    // 仅在存在具体错误码时写入 code 字段
    if let Some(code) = code {
        err["code"] = json!(code);
    }
    HttpResponse::build(status).json(json!({"error": err}))
}

/// Anthropic 风格错误响应
///
/// 将 [`AppError`] 转换为 Anthropic Messages API 兼容的错误 JSON 结构：
/// ```json
/// { "type": "error", "error": { "type": "...", "message": "..." } }
/// ```
///
/// - `e`：引擎返回的应用错误
/// - 返回值：带对应 HTTP 状态码的 [`HttpResponse`]
pub fn anthropic_error_response(e: &AppError) -> HttpResponse {
    // 上游错误：同样透传上游状态码与原始响应体（Anthropic 入站请求的上游
    // 响应体通常为 OpenAI 风格，原样透传最有利于排查）
    if matches!(e, AppError::Upstream { .. }) {
        return upstream_passthrough_response(e);
    }
    let (status, error_type, _) = error_kind(e); // Anthropic 不使用 code 字段
    HttpResponse::build(status).json(json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": error_message(e),
        }
    }))
}
