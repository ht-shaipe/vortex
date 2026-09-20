//! 应用设置业务逻辑服务。

use serde_json::Value;
use vortex_store::db::{core as db_core, settings as db_settings};

use crate::AppState;

/// 读取当前全部应用设置。
pub fn get_settings(state: &AppState) -> Result<Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    db_settings::get_settings(&conn).map_err(|e| e.to_string())
}

/// 更新应用设置，返回更新后的全部设置项。
pub fn update_settings(state: &AppState, updates: &Value) -> Result<Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    db_settings::update_settings(&conn, updates).map_err(|e| e.to_string())
}
