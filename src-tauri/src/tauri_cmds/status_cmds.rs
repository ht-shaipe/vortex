//! 系统运行状态相关 Tauri 命令。
//!
//! 本模块向前端暴露以下命令：
//! - [`get_system_status`]：获取当前系统的完整运行状态（代理状态、端口、版本、
//!   数据库状态、提供商连接数、API Key 数、今日请求量等），供托盘状态面板展示。

use crate::AppState;
use serde::Serialize;
use std::sync::Arc;

/// 系统运行状态快照。
///
/// 该结构体在托盘状态面板中展示，所有字段均为只读快照，不包含敏感信息。
#[derive(Serialize)]
pub struct SystemStatus {
    /// 代理服务器是否正在运行。
    pub proxy_running: bool,
    /// 代理服务器监听端口。
    pub proxy_port: u16,
    /// 应用版本号。
    pub version: String,
    /// 数据库是否连通正常。
    pub database_ok: bool,
    /// 已配置的提供商连接总数。
    pub provider_count: usize,
    /// 处于活跃状态的提供商连接数。
    pub active_provider_count: usize,
    /// API Key 总数。
    pub api_key_count: usize,
    /// 处于活跃且未被封禁的 API Key 数。
    pub active_api_key_count: usize,
    /// 今日累计请求数。
    pub today_requests: i64,
    /// 今日累计输入 Token 数。
    pub today_tokens_input: i64,
    /// 今日累计输出 Token 数。
    pub today_tokens_output: i64,
}

/// 获取当前系统的完整运行状态。
///
/// 从应用全局状态中读取代理运行状态与端口，从数据库查询提供商连接、
/// API Key 与今日用量统计，组装为 [`SystemStatus`] 快照返回。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态。
///
/// # 返回值
/// - 成功时返回 `Ok(SystemStatus)`。
/// - 数据库查询失败时对应字段回退为 0 / false，不中断整体返回。
#[tauri::command]
pub async fn get_system_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SystemStatus, String> {
    // 代理运行状态：proxy_handle 为 Some 即表示正在运行
    let proxy_running = state.proxy_handle.lock().is_some();
    let proxy_port = state.proxy_port;
    let version = env!("CARGO_PKG_VERSION").to_string();

    // 获取数据库连接
    let conn = match crate::db::core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(_) => {
            // 数据库不可用时返回降级状态
            return Ok(SystemStatus {
                proxy_running,
                proxy_port,
                version,
                database_ok: false,
                provider_count: 0,
                active_provider_count: 0,
                api_key_count: 0,
                active_api_key_count: 0,
                today_requests: 0,
                today_tokens_input: 0,
                today_tokens_output: 0,
            });
        }
    };

    // 数据库连通性探测
    let database_ok = conn
        .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
        .is_ok();

    // 提供商连接统计
    let (provider_count, active_provider_count) = match crate::db::providers::list(&conn, &state.encryption_key) {
        Ok(list) => {
            let active = list.iter().filter(|p| p.is_active).count();
            (list.len(), active)
        }
        Err(_) => (0, 0),
    };

    // API Key 统计
    let (api_key_count, active_api_key_count) = match crate::db::api_keys::list(&conn) {
        Ok(list) => {
            let active = list.iter().filter(|k| k.is_active && !k.is_banned).count();
            (list.len(), active)
        }
        Err(_) => (0, 0),
    };

    // 今日用量统计
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (today_requests, today_tokens_input, today_tokens_output) =
        match crate::db::usage::get_stats(&conn, Some(&today)) {
            Ok(stats) => (stats.total_requests, stats.total_tokens_input, stats.total_tokens_output),
            Err(_) => (0, 0, 0),
        };

    Ok(SystemStatus {
        proxy_running,
        proxy_port,
        version,
        database_ok,
        provider_count,
        active_provider_count,
        api_key_count,
        active_api_key_count,
        today_requests,
        today_tokens_input,
        today_tokens_output,
    })
}
