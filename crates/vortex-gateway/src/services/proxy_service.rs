//! 代理服务器启停业务逻辑服务。

use std::sync::Arc;

use crate::AppState;

/// 启动本地代理服务器。
///
/// 若代理已在运行则返回错误。
pub fn start_proxy(state: &Arc<AppState>) -> Result<(), String> {
    let mut guard = state.proxy_handle.lock();
    if guard.is_some() {
        return Err("代理已在运行".into());
    }
    let handle = crate::start_api_server(state.clone(), state.proxy_port);
    *guard = Some(handle);
    Ok(())
}

/// 停止本地代理服务器。
///
/// 若代理未运行则返回错误。需要 async runtime 以等待服务器完全关闭。
pub async fn stop_proxy(state: &Arc<AppState>) -> Result<(), String> {
    let handle = {
        let mut guard = state.proxy_handle.lock();
        guard.take()
    };
    if let Some(handle) = handle {
        handle.stop(true).await;
        Ok(())
    } else {
        Err("代理未运行".into())
    }
}
