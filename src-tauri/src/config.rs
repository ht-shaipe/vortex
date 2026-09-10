use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub data_dir: PathBuf,
    pub encryption_key: String,
    pub require_api_key: bool,
    pub log_level: String,
}

impl AppConfig {
    pub fn load() -> Self {
        let data_dir = std::env::var("VORTEX_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("vortex")
            });

        let encryption_key = std::env::var("VORTEX_ENCRYPTION_KEY")
            .unwrap_or_else(|_| {
                let key_file = data_dir.join(".encryption_key");
                if key_file.exists() {
                    std::fs::read_to_string(&key_file).unwrap_or_else(|_| generate_and_persist_key(&key_file))
                } else {
                    generate_and_persist_key(&key_file)
                }
            });

        Self {
            port: std::env::var("VORTEX_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(20128),
            data_dir,
            encryption_key,
            require_api_key: std::env::var("VORTEX_REQUIRE_API_KEY")
                .ok()
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            log_level: std::env::var("VORTEX_LOG_LEVEL")
                .unwrap_or_else(|_| "info".to_string()),
        }
    }

    pub fn encryption_key_bytes(&self) -> Vec<u8> {
        crate::db::encryption::derive_key(&self.encryption_key)
    }
}

fn generate_and_persist_key(path: &PathBuf) -> String {
    let key = crate::db::encryption::generate_encryption_key();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, &key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    key
}

mod dirs {
    use std::path::PathBuf;

    pub fn data_dir() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            if let Ok(home) = std::env::var("HOME") {
                return Some(PathBuf::from(home).join("Library").join("Application Support"));
            }
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
                return Some(PathBuf::from(xdg));
            }
            if let Ok(home) = std::env::var("HOME") {
                return Some(PathBuf::from(home).join(".local").join("share"));
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return Some(PathBuf::from(appdata));
            }
        }
        None
    }
}
