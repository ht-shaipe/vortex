//! 网关运行信息 Tauri 命令。
//!
//! 向前端暴露网关的 base 地址（scheme://host:port），使前端请求与
//! TLS 配置（VORTEX_TLS）保持同步，无需各处硬编码协议。

use vortex_gateway::AppState;

/// 返回网关 base 地址（如 `https://localhost:10168`）。
///
/// 前端启动时调用一次，用于初始化 axios / SSE / 健康检查的目标地址。
#[tauri::command]
pub async fn gateway_base_url(state: tauri::State<'_, std::sync::Arc<AppState>>) -> Result<String, String> {
    let cfg = &state.config;
    // 双端口模式：TLS 开启时前端优先走 HTTPS 端口（webview 信任本地 CA），HTTP 主端口继续保留
    if cfg.tls_enabled {
        Ok(format!("https://localhost:{}", cfg.tls_https_port))
    } else {
        Ok(format!("http://localhost:{}", cfg.port))
    }
}
