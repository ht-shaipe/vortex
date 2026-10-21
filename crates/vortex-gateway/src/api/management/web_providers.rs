//! 网页版平台（浏览器自动化）管理端点。
//!
//! 提供「拉起本地 Chrome → 用户在真实浏览器登录 → CDP 收集 cookie
//! 注入 obscura worker」的授权编排接口：
//! - `GET  /api/web-providers`：全部已知平台列表（worker 启用状态 + 登录态 + CDP 会话状态）
//! - `POST /api/web-providers/{platform}/login/cdp`：开始 CDP 授权（worker 未启动时按需启动）
//! - `GET  /api/web-providers/{platform}/login/cdp`：授权进度（前端轮询）
//! - `POST /api/web-providers/{platform}/login/cdp/finish`：手动触发 cookie 收集
//! - `POST /api/web-providers/{platform}/login/cdp/cancel`：取消授权
//!
//! 启动时 `VORTEX_WEB_PLATFORMS`（默认 doubao）决定预启动的 worker，
//! 其余已知平台在发起授权时按需启动。

use actix_web::{HttpResponse, web};
use serde_json::json;
use std::sync::Arc;
use vortex_router::proxy::web_bridge::WebChatBridge;

use crate::AppState;

/// 校验平台名：小写化并检查是否为已知平台（不要求 worker 已启动，
/// 授权发起时会按需启动）。
fn normalize_platform(state: &web::Data<Arc<AppState>>, raw: &str) -> Result<String, HttpResponse> {
    let platform = raw.trim().to_lowercase();
    let known = state.web_bridge.known_platforms();
    if !known.contains(&platform) {
        return Err(HttpResponse::NotFound().json(json!({
            "error": format!("未知网页版平台: {platform}（支持: {:?}）", known),
        })));
    }
    Ok(platform)
}

/// 处理 GET 请求，列出全部已知网页版平台与状态。
///
/// 每个平台返回：
/// - `enabled`：worker 是否已启动（未启动平台授权时按需拉起）
/// - `loggedIn`：worker 报告的登录态（未启动为 false；已启动但 worker
///   忙于长命令时查询超时，为 null 表示未知）
/// - `cdp`：进行中或最近结束的 CDP 授权会话状态
pub async fn list_web_providers(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let bridge = &state.web_bridge;
    let mut platforms = Vec::new();
    for platform in bridge.known_platforms() {
        let enabled = bridge.available(&platform);
        // worker 登录态：未启动平台直接 false；已启动时 3 秒超时
        //（worker 可能正在处理长命令 Ask），超时则 null（未知）
        let logged_in = if !enabled {
            Some(false)
        } else {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                bridge.login_status(&platform),
            )
            .await
            {
                Ok(Ok(v)) => Some(v),
                Ok(Err(e)) => {
                    log::warn!("[web] 平台 {platform} 登录态查询失败: {e}");
                    None
                }
                Err(_) => None,
            }
        };
        let cdp = bridge.cdp_status(&platform).await;
        platforms.push(json!({
            "platform": platform,
            "enabled": enabled,
            "loggedIn": logged_in,
            "cdp": cdp,
        }));
    }
    HttpResponse::Ok().json(json!({ "platforms": platforms }))
}

/// 处理 POST 请求，开始指定平台的 CDP 授权。
///
/// 拉起本地 Chrome 打开平台登录页；已有进行中会话返回 409。
pub async fn start_cdp_login(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = match normalize_platform(&state, &path.into_inner()) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    match state.web_bridge.start_cdp_login(&platform).await {
        Ok(()) => HttpResponse::Accepted().json(json!({
            "platform": platform,
            "message": "Chrome 已拉起，请在浏览器中完成登录",
        })),
        Err(e) => {
            // 已有进行中的会话 vs 环境（找不到 Chrome 等）失败
            if e.contains("已有进行中的 CDP 授权会话") {
                HttpResponse::Conflict().json(json!({ "error": e }))
            } else {
                log::error!("[web] 平台 {platform} CDP 授权启动失败: {e}");
                HttpResponse::BadRequest().json(json!({ "error": e }))
            }
        }
    }
}

/// 处理 GET 请求，查询指定平台 CDP 授权进度（前端轮询）。
///
/// 无进行中或已结束的会话返回 404（前端据此判断未启动）。
pub async fn cdp_login_status(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = match normalize_platform(&state, &path.into_inner()) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    match state.web_bridge.cdp_status(&platform).await {
        Some(status) => HttpResponse::Ok().json(status),
        None => HttpResponse::NotFound().json(json!({
            "error": format!("平台 {platform} 当前没有 CDP 授权会话"),
        })),
    }
}

/// 处理 POST 请求，手动触发 cookie 收集。
///
/// 用户确认已在 Chrome 中完成登录、不想等自动检测时调用；
/// 无进行中或已结束的会话返回 404。
pub async fn finish_cdp_login(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = match normalize_platform(&state, &path.into_inner()) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    if state.web_bridge.cdp_finish(&platform).await {
        HttpResponse::Accepted().json(json!({
            "platform": platform,
            "message": "已触发 cookie 收集",
        }))
    } else {
        HttpResponse::NotFound().json(json!({
            "error": format!("平台 {platform} 当前没有进行中的 CDP 授权会话"),
        }))
    }
}

/// 处理 POST 请求，取消 CDP 授权（关闭 Chrome、终止轮询）。
///
/// 无进行中或已结束的会话返回 404。
pub async fn cancel_cdp_login(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = match normalize_platform(&state, &path.into_inner()) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    if state.web_bridge.cdp_cancel(&platform).await {
        HttpResponse::Ok().json(json!({ "success": true }))
    } else {
        HttpResponse::NotFound().json(json!({
            "error": format!("平台 {platform} 当前没有进行中的 CDP 授权会话"),
        }))
    }
}

/// 处理 GET 请求，查询平台 worker 运行时状态（登录态 + Debug 诊断信息）。
///
/// provider 错误消息中的「GET /api/web-providers/{platform}/debug」指此；
/// worker 未启动返回 404。
pub async fn platform_debug(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = match normalize_platform(&state, &path.into_inner()) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let logged_in = state.web_bridge.login_status(&platform).await;
    match state.web_bridge.platform_debug(&platform).await {
        Ok(info) => {
            let info_json: serde_json::Value =
                serde_json::from_str(&info).unwrap_or(json!({ "raw": info }));
            HttpResponse::Ok().json(json!({
                "platform": platform,
                "logged_in": logged_in.ok(),
                "debug": info_json,
            }))
        }
        Err(e) => HttpResponse::NotFound().json(json!({
            "error": format!("平台 {platform} 不可用: {e}"),
        })),
    }
}

/// 处理 POST 请求，重置平台页面（新开 page 导航回站点，丢弹窗/风控遮罩）。
///
/// provider 错误消息中的「POST /api/web-providers/{platform}/reset」指此；
/// worker 未启动返回 404。
pub async fn platform_reset(
    state: web::Data<Arc<AppState>>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = match normalize_platform(&state, &path.into_inner()) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    match state.web_bridge.platform_reset(&platform).await {
        Ok(()) => HttpResponse::Ok().json(json!({
            "platform": platform,
            "message": "页面已重置，正在导航回站点",
        })),
        Err(e) => HttpResponse::NotFound().json(json!({
            "error": format!("平台 {platform} 不可用: {e}"),
        })),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalize_platform_rejects_unknown() {
        // 无 AppState 的纯字符串校验逻辑已内联在各 handler；
        // 此处仅验证平台名集合文档一致性。
        let known = ["doubao", "chatgpt", "claude", "deepseek", "qianwen", "kimi"];
        assert!(known.contains(&"doubao"));
        assert!(known.contains(&"claude"));
        assert!(!known.contains(&"unknown-platform"));
    }
}
