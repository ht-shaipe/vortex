//! 应用设置相关 Tauri 命令。
//!
//! 本模块向前端暴露以下命令：
//! - [`get_settings`]：读取当前应用设置
//! - [`update_settings`]：更新应用设置

use crate::db::{core as db_core, settings as db_settings};
use crate::AppState;
use std::sync::Arc;

/// 读取当前应用设置。
///
/// 从数据库中查询并返回全部应用设置项，以 JSON 对象形式提供给前端。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态，包含数据库连接池。
///
/// # 返回值
/// 返回包含全部设置项的 JSON 对象。
///
/// # 错误
/// 若获取数据库连接或读取设置失败，则返回对应的错误字符串。
#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // 获取数据库连接
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    // 从数据库读取全部设置项
    db_settings::get_settings(&conn).map_err(|e| e.to_string())
}

/// 更新应用设置。
///
/// 接收一个 JSON 对象作为更新内容，将其合并写入数据库中的设置表。
///
/// # 参数
/// - `state`：Tauri 管理的全局应用状态。
/// - `updates`：包含待更新设置项的 JSON 对象，键为设置名，值为设置值。
///
/// # 返回值
/// 返回更新后的全部设置项（JSON 对象）。
///
/// # 错误
/// 若获取数据库连接或更新设置失败，则返回对应的错误字符串。
#[tauri::command]
pub async fn update_settings(
    state: tauri::State<'_, Arc<AppState>>,
    updates: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // 获取数据库连接
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    // 将更新内容写入数据库并返回更新后的设置
    db_settings::update_settings(&conn, &updates).map_err(|e| e.to_string())
}
