//! 远程接口代理（内部模块）
//!
//! 移植自 `dsa` 项目的通用外部 API 代理能力：对出站 HTTP 请求做域名白名单校验后转发。
//! 与 `dsa` 不同，此处仅作为内部函数供业务处理器调用（如免费 Token 的远程读写），
//! 不暴露通用的 `POST /api/proxy` 端点，攻击面更小。

use std::collections::HashMap;
use std::time::Duration;

use crate::error::{AppError, Result};
use log::{info, warn};

/// 允许被代理的域名白名单（子域同样放行）。可按需追加。
const ALLOWED_DOMAINS: &[&str] = &["hub.htui.cc"];


/// 从 URL 中提取主机名（去除协议前缀和路径）
fn host_of(url: &str) -> Option<String> {
    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    trimmed.split('/').next().map(|h| h.to_string())
}

/// 校验 URL 主机是否在白名单内（精确匹配或子域匹配）
pub fn is_url_allowed(url: &str) -> bool {
    match host_of(url) {
        Some(h) => ALLOWED_DOMAINS
            .iter()
            .any(|d| h == *d || h.ends_with(&format!(".{}", d))),
        None => false,
    }
}

/// 转发一次出站请求，复用调用方传入的 `awc::Client`。
///
/// - `url`：目标地址，必须落在 `ALLOWED_DOMAINS` 内，否则返回 `BadRequest`
/// - `method`：GET / POST / PUT / DELETE / PATCH（其它按 POST 处理）
/// - `headers`：附加请求头
/// - `body`：请求体（设置时默认 `Content-Type: application/json`，可被 `headers` 覆盖）
/// - `timeout_secs`：本次请求超时（秒）
///
/// 成功时返回解析后的远程 JSON；远程状态码 >= 400 或解析失败则返回 `AppError`。
pub async fn send_proxy_request(
    client: &awc::Client,
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
    timeout_secs: u64,
) -> Result<serde_json::Value> {
    if !is_url_allowed(url) {
        return Err(AppError::BadRequest(format!("域名不在白名单，禁止代理: {}", url)));
    }

    info!("[PROXY] {} {}", method.to_uppercase(), url);

    let req_method = match method.to_uppercase().as_str() {
        "GET" => awc::http::Method::GET,
        "PUT" => awc::http::Method::PUT,
        "DELETE" => awc::http::Method::DELETE,
        "PATCH" => awc::http::Method::PATCH,
        _ => awc::http::Method::POST,
    };

    let mut req = client.request(req_method, url);
    for (k, v) in headers {
        req = req.insert_header((k.as_str(), v.as_str()));
    }

    let send_fut = if let Some(b) = body {
        let ct = headers
            .get("Content-Type")
            .map(|s| s.as_str())
            .unwrap_or("application/json");
        req.insert_header((awc::http::header::CONTENT_TYPE, ct))
            .send_body(b.to_owned())
    } else {
        req.send()
    };

    let mut resp = actix_web::rt::time::timeout(Duration::from_secs(timeout_secs), send_fut)
        .await
        .map_err(|_| AppError::BadRequest(format!("远程请求超时（{}s）", timeout_secs)))?
        .map_err(AppError::Http)?;

    let status = resp.status();
    let bytes = resp
        .body()
        .await
        .map_err(|e| AppError::Internal(format!("读取响应失败: {e}")))?;
    let text = String::from_utf8_lossy(&bytes);

    if status.as_u16() >= 400 {
        warn!("[PROXY] 远程返回错误 {}: {}", status, text);
        return Err(AppError::BadRequest(format!(
            "远程返回错误 {}: {}",
            status, text
        )));
    }

    serde_json::from_str(&text).map_err(AppError::Json)
}
