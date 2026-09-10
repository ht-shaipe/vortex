use crate::db::{core as db_core, combos as db_combos};
use crate::db::models::{ComboStep, CreateComboRequest};
use crate::AppState;
use serde_json::json;
use std::sync::Arc;

#[tauri::command]
pub async fn list_combos(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let combos = db_combos::list(&conn).map_err(|e| e.to_string())?;
    Ok(json!({"combos": combos}))
}

#[tauri::command]
pub async fn save_combo(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    strategy: Option<String>,
    models: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let parsed_models: Option<Vec<ComboStep>> = models
        .and_then(|v| serde_json::from_value(v).ok());
    let req = CreateComboRequest {
        name,
        strategy,
        models: parsed_models,
        config: None,
        system_message: None,
    };
    let created = db_combos::create(&conn, &req).map_err(|e| e.to_string())?;
    serde_json::to_value(created).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_combo(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    let conn = db_core::get_conn(&state.db_pool).map_err(|e| e.to_string())?;
    let deleted = db_combos::delete(&conn, &id).map_err(|e| e.to_string())?;
    Ok(json!({"success": deleted}))
}
