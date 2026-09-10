use crate::AppState;
use std::sync::Arc;

#[tauri::command]
pub async fn start_proxy(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut guard = state.proxy_handle.lock();
    if guard.is_some() {
        return Err("代理已在运行".into());
    }
    let handle = crate::start_api_server(state.inner().clone(), state.proxy_port);
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
pub async fn stop_proxy(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
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