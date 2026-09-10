use crate::db::{core as db_core, settings as db_settings};
use crate::AppState;
use std::sync::Arc;

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    db_settings::get_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_settings(
    state: tauri::State<'_, Arc<AppState>>,
    updates: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    db_settings::update_settings(&conn, &updates).map_err(|e| e.to_string())
}
