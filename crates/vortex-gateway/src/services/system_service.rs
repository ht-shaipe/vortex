//! 系统运行状态业务逻辑服务。

use serde::Serialize;
use vortex_store::db;

use crate::AppState;

/// 系统运行状态快照。
#[derive(Serialize)]
pub struct SystemStatus {
    pub proxy_running: bool,
    pub proxy_port: u16,
    pub version: String,
    pub database_ok: bool,
    pub provider_count: usize,
    pub active_provider_count: usize,
    pub api_key_count: usize,
    pub active_api_key_count: usize,
    pub today_requests: i64,
    pub today_tokens_input: i64,
    pub today_tokens_output: i64,
}

/// 获取当前系统的完整运行状态。
pub fn get_system_status(state: &AppState) -> Result<SystemStatus, String> {
    let proxy_running = state.proxy_handle.lock().is_some();
    let proxy_port = state.proxy_port;
    let version = env!("CARGO_PKG_VERSION").to_string();

    let conn = match db::core::get_conn(&state.db_pool) {
        Ok(c) => c,
        Err(_) => {
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

    let database_ok = conn
        .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
        .is_ok();

    let (provider_count, active_provider_count) = match db::providers::list(&conn, &state.encryption_key) {
        Ok(list) => {
            let active = list.iter().filter(|p| p.is_active).count();
            (list.len(), active)
        }
        Err(_) => (0, 0),
    };

    let (api_key_count, active_api_key_count) = match db::api_keys::list(&conn) {
        Ok(list) => {
            let active = list.iter().filter(|k| k.is_active && !k.is_banned).count();
            (list.len(), active)
        }
        Err(_) => (0, 0),
    };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (today_requests, today_tokens_input, today_tokens_output) =
        match db::usage::get_stats(&conn, Some(&today)) {
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
