use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Routing error: {0}")]
    Routing(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl actix_web::ResponseError for AppError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => actix_web::http::StatusCode::BAD_REQUEST,
            AppError::Unauthorized(_) => actix_web::http::StatusCode::UNAUTHORIZED,
            _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        let status = self.status_code();
        let body = serde_json::json!({
            "status": status.as_u16(),
            "code": match self {
                AppError::NotFound(_) => "NOT_FOUND",
                AppError::BadRequest(_) => "BAD_REQUEST",
                AppError::Unauthorized(_) => "UNAUTHORIZED",
                AppError::Provider(_) => "PROVIDER_ERROR",
                AppError::Routing(_) => "ROUTING_ERROR",
                _ => "INTERNAL_ERROR",
            },
            "message": self.to_string(),
        });
        actix_web::HttpResponse::build(status).json(body)
    }
}
