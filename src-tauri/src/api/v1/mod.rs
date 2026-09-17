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
use actix_web::http::header::HeaderMap;
use crate::error::AppError;
use serde_json::json;

/// 从请求头识别调用方智能体（agent 指纹）。
///
/// 各 CLI 智能体在请求头中留有独特标识：Codex 带 `originator` 头，
/// 其余靠 User-Agent 特征前缀。识别结果用于 auto 路由的跨智能体
/// 模型占用避让；未识别时回退解析 UA 产品名（见 [`parse_agent_from_ua`]），
/// 仍无法归类的返回 None（不参与避让判定）。
pub fn detect_agent(headers: &HeaderMap) -> Option<String> {
    // Codex CLI 专有头：originator: codex_cli_rs
    if let Some(originator) = headers.get("originator").and_then(|v| v.to_str().ok()) {
        let o = originator.to_ascii_lowercase();
        if o.contains("codex") {
            return Some("codex".into());
        }
    }
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    // 按特异度从高到低匹配 UA 前缀/子串
    let detected = if ua.starts_with("claude-cli") {
        Some("claude_code".into())
    } else if ua.starts_with("zcode") {
        Some("zcode".into())
    } else if ua.contains("opencode") {
        Some("opencode".into())
    } else if ua.contains("qwen") {
        Some("qwen_code".into())
    } else if ua.contains("dsh") || ua.contains("deepseek") {
        Some("dsh".into())
    } else if ua.contains("codex") {
        Some("codex".into())
    } else {
        None
    };
    // 未命中已知指纹时，回退按 UA 首个产品标识归类，
    // 使未知 CLI（如 workbuddy/5.5.6 cli/2.137.1）也能参与占用避让
    let detected = detected.or_else(|| parse_agent_from_ua(&ua));
    if detected.is_none() {
        // 诊断：未识别的调用方打印 UA，便于扩充指纹规则
        if ua.is_empty() {
            log::info!("agent 指纹：请求未携带 User-Agent 头（originator 未命中），视为未知来源");
        } else {
            log::info!("agent 指纹：未识别的 User-Agent: {}", ua);
        }
    }
    detected
}

/// 从未命中已知规则的 User-Agent 中提取产品名作为 agent 标识。
///
/// UA 首个空白分隔的 token 通常是产品名（形如 `name/version`），取 `/` 前
/// 的部分并规范化为 snake_case（如 `workbuddy/5.5.6 ... cli/2.137.1` →
/// `workbuddy`）。通用 HTTP 客户端库（浏览器、python-requests、curl 等）
/// 不携带智能体语义，返回 None 保持匿名，避免把“工具链”当成调用方。
fn parse_agent_from_ua(ua: &str) -> Option<String> {
    // 首个 token 的 `/` 前部分即产品名
    let first = ua.split_whitespace().next()?;
    let product = first.split('/').next().unwrap_or(first);
    // 非 ASCII 字母数字折叠为单下划线，去首尾下划线
    let mut name = String::with_capacity(product.len());
    let mut last_underscore = false;
    for ch in product.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch);
            last_underscore = false;
        } else if !last_underscore {
            name.push('_');
            last_underscore = true;
        }
    }
    let name = name.trim_matches('_');
    // 纯数字（版本号）或无字母的标识不具备产品名语义
    let has_alpha = name.chars().any(|c| c.is_ascii_alphabetic());
    if name.len() < 2 || !has_alpha || is_generic_client(name) {
        return None;
    }
    log::info!("agent 指纹：按 UA 产品名归为 {}（UA: {}）", name, ua);
    Some(name.to_string())
}

/// 判断是否为通用 HTTP 客户端库的产品名（含前缀变体，如 python_requests）。
fn is_generic_client(name: &str) -> bool {
    const GENERIC: &[&str] = &[
        "mozilla", "python", "requests", "urllib", "aiohttp", "httpx", "go", "java",
        "okhttp", "axios", "node", "node_fetch", "undici", "got", "curl", "wget",
        "httpclient", "apache", "kotlin", "ruby", "php", "perl", "libwww", "openssl",
        "postman", "insomnia", "httpie", "swift", "cfnetwork", "darwin", "dart", "guzzle",
        "grpc", "restclient", "rest_client", "spring", "dartdio", "dio", "unityplayer",
    ];
    GENERIC.iter().any(|&g| {
        name == g
            || (name.len() > g.len() && name.starts_with(g) && name.as_bytes()[g.len()] == b'_')
    })
}

#[cfg(test)]
mod agent_tests {
    use super::*;

    fn detect_with_ua(ua: &str) -> Option<String> {
        let mut headers = HeaderMap::new();
        headers.insert(
            actix_web::http::header::HeaderName::from_static("user-agent"),
            ua.parse().unwrap(),
        );
        detect_agent(&headers)
    }

    #[test]
    fn fallback_extracts_product_name() {
        // workbuddy 这类未知 CLI：取首个产品标识，参与占用避让
        assert_eq!(
            detect_with_ua("workbuddy/5.5.6 workbuddy/5.5.6 cli/2.137.1"),
            Some("workbuddy".into())
        );
    }

    #[test]
    fn fallback_normalizes_separators() {
        // 连字符/下划线/点统一折叠为单下划线
        assert_eq!(detect_with_ua("My-Cool.CLI/1.0"), Some("my_cool_cli".into()));
    }

    #[test]
    fn fallback_ignores_generic_libraries() {
        assert_eq!(detect_with_ua("python-requests/2.31.0"), None);
        assert_eq!(detect_with_ua("Python/3.11 aiohttp/3.9.1"), None);
        assert_eq!(detect_with_ua("Go-http-client/2.0"), None);
        assert_eq!(detect_with_ua("curl/8.4.0"), None);
        assert_eq!(detect_with_ua("node"), None);
        assert_eq!(detect_with_ua("Mozilla/5.0 (Macintosh)"), None);
    }

    #[test]
    fn fallback_rejects_noise() {
        // 纯版本号、单字符等无意义标识
        assert_eq!(detect_with_ua("1.0.0"), None);
        assert_eq!(detect_with_ua("x/1"), None);
        assert_eq!(detect_with_ua(""), None);
    }

    #[test]
    fn known_rules_still_win() {
        // 已知指纹不受 fallback 影响
        assert_eq!(
            detect_with_ua("claude-cli/1.0.23 (external)"),
            Some("claude_code".into())
        );
        assert_eq!(detect_with_ua("Zcode/2.3.0"), Some("zcode".into()));
    }
}

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
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body)
        && v.is_object() {
            return HttpResponse::build(status_code)
                .content_type("application/json")
                .body(body.clone());
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
