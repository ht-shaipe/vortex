//! 代理启停相关 Tauri 命令。
//!
//! 本模块向前端暴露以下命令：
//! - [`start_proxy`]：启动本地代理服务器
//! - [`stop_proxy`]：停止本地代理服务器

use crate::AppState;
use std::sync::Arc;

/// 启动本地代理服务器。
///
/// 检查代理是否已在运行，若未运行则使用应用状态中配置的端口启动 API 服务器，
/// 并将服务器句柄保存到全局状态中以便后续停止。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态，包含代理句柄锁和代理端口。
///
/// # 返回值
/// - 成功时返回 `Ok(())`。
/// - 若代理已在运行，则返回 `Err("代理已在运行")`。
///
/// # 错误
/// 若代理已在运行，则返回错误字符串 `"代理已在运行"`。
#[tauri::command]
pub async fn start_proxy(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    // 获取代理句柄的互斥锁
    let mut guard = state.proxy_handle.lock();
    // 若代理已在运行则直接返回错误
    if guard.is_some() {
        return Err("代理已在运行".into());
    }
    // 启动 API 服务器，传入应用状态克隆和代理端口
    let handle = crate::start_api_server(state.inner().clone(), state.proxy_port);
    // 保存服务器句柄以便后续停止
    *guard = Some(handle);
    Ok(())
}

/// 停止本地代理服务器。
///
/// 从全局状态中取出代理服务器句柄，若存在则停止服务器；若不存在则返回错误。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态，包含代理句柄锁。
///
/// # 返回值
/// - 成功时返回 `Ok(())`。
/// - 若代理未运行，则返回 `Err("代理未运行")`。
///
/// # 错误
/// 若代理未运行，则返回错误字符串 `"代理未运行"`。
#[tauri::command]
pub async fn stop_proxy(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    // 从互斥锁中取出代理句柄（take 会将原值替换为 None）
    let handle = {
        let mut guard = state.proxy_handle.lock();
        guard.take()
    };
    if let Some(handle) = handle {
        // 停止服务器，参数 true 表示等待服务器完全关闭
        handle.stop(true).await;
        Ok(())
    } else {
        // 代理未运行
        Err("代理未运行".into())
    }
}
