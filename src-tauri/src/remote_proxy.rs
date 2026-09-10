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

/// 伪装的 User-Agent 字符串，模拟主流浏览器请求
const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

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

/// 转发一次出站请求，复用调用方传入的 `reqwest::Client`。
///
/// - `url`：目标地址，必须落在 `ALLOWED_DOMAINS` 内，否则返回 `BadRequest`
/// - `method`：GET / POST / PUT / DELETE / PATCH（其它按 POST 处理）
/// - `headers`：附加请求头
/// - `body`：请求体（设置时默认 `Content-Type: application/json`，可被 `headers` 覆盖）
/// - `timeout_secs`：本次请求超时（秒）
///
/// 成功时返回解析后的远程 JSON；远程状态码 >= 400 或解析失败则返回 `AppError`。
pub async fn send_proxy_request(
    client: &reqwest::Client,
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
        "GET" => reqwest::Method::GET,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::POST,
    };

    let mut rb = client
        .request(req_method, url)
        .timeout(Duration::from_secs(timeout_secs))
        .header(reqwest::header::USER_AGENT, UA);

    for (k, v) in headers {
        rb = rb.header(k.as_str(), v.as_str());
    }

    if let Some(b) = body {
        let ct = headers
            .get("Content-Type")
            .map(|s| s.as_str())
            .unwrap_or("application/json");
        rb = rb.header(reqwest::header::CONTENT_TYPE, ct).body(b.to_owned());
    }

    let resp = rb.send().await.map_err(AppError::Http)?;
    let status = resp.status();
    let text = resp.text().await.map_err(AppError::Http)?;

    if status.as_u16() >= 400 {
        warn!("[PROXY] 远程返回错误 {}: {}", status, text);
        return Err(AppError::BadRequest(format!(
            "远程返回错误 {}: {}",
            status, text
        )));
    }

    serde_json::from_str(&text).map_err(AppError::Json)
}
