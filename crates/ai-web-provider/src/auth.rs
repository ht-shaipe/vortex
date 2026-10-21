//! Bearer Token 认证中间件。
//!
//! `API_TOKEN` 环境变量非空时启用：`/api/*` 与 `/v1/*` 请求必须携带
//! `Authorization: Bearer <token>`，否则返回 401。
//! `/healthz` 豁免（健康检查不带凭据）。未设置 `API_TOKEN` 时全部放行。
//!
//! 认证 token 从 [`crate::server::AppState`] 的 `token` 字段读取，
//! 因此中间件必须在挂载 `web::Data<AppState>` 之后生效。

use actix_web::body::BoxBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web, Error, HttpResponse};
use futures::future::{ready, LocalBoxFuture};

/// Actix 中间件工厂：为每个 worker 构造 [`RequireAuthMiddleware`]。
pub struct RequireAuth;

impl<S> Transform<S, ServiceRequest> for RequireAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = RequireAuthMiddleware<S>;
    type InitError = ();
    type Future = futures::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequireAuthMiddleware { service }))
    }
}

/// 实际执行认证检查的中间件服务。
pub struct RequireAuthMiddleware<S> {
    service: S,
}

impl<S> Service<ServiceRequest> for RequireAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let exempt = req.path() == "/healthz";
        let token_required = req
            .app_data::<web::Data<crate::server::AppState>>()
            .and_then(|state| state.token.clone())
            .filter(|_| !exempt);

        let authorized = match token_required {
            None => true,
            Some(expected) => req
                .headers()
                .get(actix_web::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .is_some_and(|t| t == expected),
        };

        if authorized {
            Box::pin(self.service.call(req))
        } else {
            Box::pin(async move {
                let (http_req, _) = req.into_parts();
                let res = HttpResponse::Unauthorized()
                    .json(serde_json::json!({
                        "error": "unauthorized: missing or invalid API token"
                    }))
                    .map_into_boxed_body();
                Ok(ServiceResponse::new(http_req, res))
            })
        }
    }
}
