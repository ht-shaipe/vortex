//! 应用错误处理模块。
//!
//! 定义统一的错误枚举 [`AppError`]，涵盖数据库、连接池、序列化、HTTP 客户端、
//! 加密、未找到、非法请求、未授权、提供商、路由、IO 及内部错误等多种错误类型。
//! 该枚举实现了 [`actix_web::ResponseError`] trait，可将错误自动转换为 HTTP 响应，
//! 并按错误类型映射到对应的 HTTP 状态码与错误码。

use thiserror::Error;

/// 应用统一错误类型。
///
/// 通过 `thiserror` 派生宏自动实现 `std::error::Error` 和 `Display`，
/// 各变体的 `#[from]` 属性支持从底层错误类型自动转换（`?` 运算符）。
#[derive(Error, Debug)]
pub enum AppError {
    /// 数据库错误，源自 SQLite（rusqlite）。
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    /// 连接池错误，源自 r2d2。
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),
    /// 序列化/反序列化错误，源自 serde_json。
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    /// HTTP 客户端错误，源自 reqwest。
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    /// 加密/解密错误，以字符串描述具体原因。
    #[error("Encryption error: {0}")]
    Encryption(String),
    /// 资源未找到错误，映射为 HTTP 404。
    #[error("Not found: {0}")]
    NotFound(String),
    /// 非法请求错误，映射为 HTTP 400。
    #[error("Bad request: {0}")]
    BadRequest(String),
    /// 未授权错误，映射为 HTTP 401。
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    /// 上游 AI 提供商返回的错误。
    #[error("Provider error: {0}")]
    Provider(String),
    /// 上游返回的非 2xx 响应，透传给下游以便排查。
    ///
    /// 携带上游状态码与原始响应体。下游客户端将收到与上游一致的状态码，
    /// 响应体优先原样透传（上游本身就是 OpenAI/Anthropic 风格错误 JSON）；
    /// 非 JSON 响应体则包装为 OpenAI 风格错误。
    #[error("Upstream returned {status}: {body}")]
    Upstream { status: u16, body: String },
    /// 熔断器处于打开态，请求被快速失败拦截。
    ///
    /// 携带熔断器名称（`provider_id:connection_id`）。此前调用方依赖错误文案中的
    /// "Circuit breaker open" 字符串匹配来判断熔断，一旦文案调整就会静默失效，
    /// 因此改为结构化变体，由调用方通过模式匹配识别。
    #[error("Circuit breaker open for {0}")]
    CircuitOpen(String),
    /// 路由策略相关错误。
    #[error("Routing error: {0}")]
    Routing(String),
    /// 输入/输出错误，源自标准库。
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// 其他内部错误，映射为 HTTP 500。
    #[error("{0}")]
    Internal(String),
}

/// 应用统一 Result 类型别名，固定错误类型为 [`AppError`]。
pub type Result<T> = std::result::Result<T, AppError>;

/// 为 [`AppError`] 实现 Actix Web 的 `ResponseError` trait，
/// 使其能被自动转换为 HTTP 响应返回给客户端。
impl actix_web::ResponseError for AppError {
    /// 根据错误类型返回对应的 HTTP 状态码。
    ///
    /// - [`AppError::NotFound`] → 404
    /// - [`AppError::BadRequest`] → 400
    /// - [`AppError::Unauthorized`] → 401
    /// - 其他错误 → 500
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => actix_web::http::StatusCode::BAD_REQUEST,
            AppError::Unauthorized(_) => actix_web::http::StatusCode::UNAUTHORIZED,
            // 上游错误透传上游状态码（无法解析时回退 502）
            AppError::Upstream { status, .. } => {
                actix_web::http::StatusCode::from_u16(*status)
                    .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY)
            }
            _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 将错误转换为 JSON 格式的 HTTP 响应体。
    ///
    /// 响应体包含 `status`（状态码数值）、`code`（错误码字符串）和 `message`（错误描述）三个字段。
    fn error_response(&self) -> actix_web::HttpResponse {
        let status = self.status_code();
        // 构造 JSON 错误响应体
        let body = serde_json::json!({
            "status": status.as_u16(),
            "code": match self {
                AppError::NotFound(_) => "NOT_FOUND",
                AppError::BadRequest(_) => "BAD_REQUEST",
                AppError::Unauthorized(_) => "UNAUTHORIZED",
                AppError::Provider(_) => "PROVIDER_ERROR",
                AppError::Upstream { .. } => "UPSTREAM_ERROR",
                AppError::Routing(_) => "ROUTING_ERROR",
                AppError::CircuitOpen(_) => "CIRCUIT_OPEN",
                _ => "INTERNAL_ERROR",
            },
            "message": self.to_string(),
        });
        // 以指定状态码返回 JSON 响应
        actix_web::HttpResponse::build(status).json(body)
    }
}
