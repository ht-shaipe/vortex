use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDef {
    pub id: String,
    pub alias: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub service_kinds: Vec<String>,
    pub no_auth: bool,
    pub has_free: bool,
    pub free_note: Option<String>,
    pub auth_hint: Option<String>,
    pub base_url: String,
    pub chat_path: String,
    pub models_path: String,
    pub api_format: String,
    pub auth_type: String,
}
