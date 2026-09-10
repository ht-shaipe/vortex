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
/// GET /v1/models 模型列表端点
pub mod models;
/// POST /v1/embeddings 文本嵌入端点
pub mod embeddings;
/// POST /v1/images/generations 图像生成端点
pub mod images;

use actix_web::HttpResponse;
use crate::error::AppError;
use serde_json::json;

/// 将引擎错误映射为 (HTTP 状态码, OpenAI 错误类型, 错误码)。
///
/// 该函数是 [`openai_error_response`] 与 [`anthropic_error_response`] 的共享底层，
/// 负责把内部 [`AppError`] 枚举映射为 HTTP 层可用的状态码与错误分类字符串。
///
/// - `BadRequest` → 400 / `invalid_request_error`
/// - `Unauthorized` → 401 / `authentication_error`
/// - `Routing` → 404 / `invalid_request_error` + `model_not_found`
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
    let (status, error_type, code) = error_kind(e); // 解构状态码、类型、可选错误码
    let mut err = json!({
        "message": e.to_string(),
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
    let (status, error_type, _) = error_kind(e); // Anthropic 不使用 code 字段
    HttpResponse::build(status).json(json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": e.to_string(),
        }
    }))
}
