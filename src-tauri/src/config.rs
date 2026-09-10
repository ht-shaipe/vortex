//! 应用配置管理模块。
//!
//! 定义 [`AppConfig`] 结构体，负责从环境变量加载运行时配置，包括服务端口、
//! 数据目录、加密密钥、是否要求 API Key、日志级别以及免费 Token 远程服务地址。
//! 加密密钥支持从环境变量指定或自动生成并持久化到数据目录。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 应用运行时配置。
///
/// 通过 [`AppConfig::load`] 从环境变量加载，未设置的环境变量使用默认值。
/// 该结构体可被序列化/反序列化，用于在 Tauri 命令与前端之间传递配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// API 服务器监听端口，默认 `20128`。
    pub port: u16,
    /// 数据存储目录路径，用于 SQLite 数据库与加密密钥文件。
    pub data_dir: PathBuf,
    /// 加密密钥的原始字符串，用于加密存储中的敏感数据（如 API Key）。
    pub encryption_key: String,
    /// 是否要求请求携带 API Key 进行鉴权，默认 `false`。
    pub require_api_key: bool,
    /// 日志级别，默认 `"info"`。
    pub log_level: String,
    /// 免费 Token 的远程服务基址（列表 GET 与提交 POST 共用）。
    /// 为空时免费 Token 完全走本地 SQLite；非空时列表与提交改为远程。
    pub free_tokens_remote: String,
}

impl AppConfig {
    /// 从环境变量加载应用配置。
    ///
    /// # 环境变量
    ///
    /// | 变量名 | 说明 | 默认值 |
    /// |--------|------|--------|
    /// | `VORTEX_DATA_DIR` | 数据目录 | 系统数据目录下的 `vortex` 子目录 |
    /// | `VORTEX_ENCRYPTION_KEY` | 加密密钥 | 自动生成并持久化到数据目录 |
    /// | `VORTEX_PORT` | 服务端口 | `20128` |
    /// | `VORTEX_REQUIRE_API_KEY` | 是否要求 API Key | `false` |
    /// | `VORTEX_LOG_LEVEL` | 日志级别 | `info` |
    /// | `VORTEX_FREE_TOKENS_REMOTE` | 免费 Token 远程服务地址 | `https://hub.htui.cc/api/edge/free_tokens` |
    ///
    /// # 返回值
    ///
    /// 返回加载完成的 [`AppConfig`] 实例。
    pub fn load() -> Self {
        // 读取数据目录：优先使用环境变量，否则回退到系统数据目录下的 vortex 子目录
        let data_dir = std::env::var("VORTEX_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("vortex")
            });

        // 读取加密密钥：优先使用环境变量，否则从密钥文件读取或自动生成
        let encryption_key = std::env::var("VORTEX_ENCRYPTION_KEY")
            .unwrap_or_else(|_| {
                let key_file = data_dir.join(".encryption_key");
                if key_file.exists() {
                    // 密钥文件已存在，读取其中的密钥
                    std::fs::read_to_string(&key_file).unwrap_or_else(|_| generate_and_persist_key(&key_file))
                } else {
                    // 密钥文件不存在，生成新密钥并持久化
                    generate_and_persist_key(&key_file)
                }
            });

        Self {
            // 解析端口，失败时使用默认端口 20128
            port: std::env::var("VORTEX_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(20128),
            data_dir,
            encryption_key,
            // 判断是否要求 API Key：值为 "true" 或 "1" 时为真
            require_api_key: std::env::var("VORTEX_REQUIRE_API_KEY")
                .ok()
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            // 日志级别默认 info
            log_level: std::env::var("VORTEX_LOG_LEVEL")
                .unwrap_or_else(|_| "info".to_string()),
            // 免费 Token 远程服务地址
            free_tokens_remote: std::env::var("VORTEX_FREE_TOKENS_REMOTE")
                .unwrap_or_else(|_| "https://hub.htui.cc/api/edge/free_tokens".to_string()),
        }
    }

    /// 将加密密钥字符串派生为字节序列。
    ///
    /// 内部调用 [`crate::db::encryption::derive_key`] 进行密钥派生（如哈希/填充），
    /// 返回可用于实际加密操作的字节向量。
    ///
    /// # 返回值
    ///
    /// 返回派生后的加密密钥字节向量。
    pub fn encryption_key_bytes(&self) -> Vec<u8> {
        crate::db::encryption::derive_key(&self.encryption_key)
    }
}

/// 生成新的加密密钥并持久化到指定文件路径。
///
/// 当密钥文件不存在或读取失败时调用此函数。生成后会创建必要的父目录，
/// 并在 Unix 系统上将文件权限设置为 `0600`（仅所有者可读写）以保证安全性。
///
/// # 参数
///
/// - `path`：密钥文件的保存路径。
///
/// # 返回值
///
/// 返回生成并已持久化的加密密钥字符串。
fn generate_and_persist_key(path: &PathBuf) -> String {
    // 生成新的随机加密密钥
    let key = crate::db::encryption::generate_encryption_key();
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 将密钥写入文件
    let _ = std::fs::write(path, &key);
    // 在 Unix 系统上设置文件权限为 0600（仅所有者可读写）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    key
}

/// 跨平台数据目录获取模块。
///
/// 根据操作系统返回对应的应用数据目录路径：
/// - macOS：`~/Library/Application Support`
/// - Linux：`$XDG_DATA_HOME` 或 `~/.local/share`
/// - Windows：`%APPDATA%`
mod dirs {
    use std::path::PathBuf;

    /// 获取当前平台的应用数据目录路径。
    ///
    /// # 返回值
    ///
    /// 返回 `Some(PathBuf)` 表示成功获取，`None` 表示无法确定（如环境变量未设置）。
    pub fn data_dir() -> Option<PathBuf> {
        // macOS：~/Library/Application Support
        #[cfg(target_os = "macos")]
        {
            if let Ok(home) = std::env::var("HOME") {
                return Some(PathBuf::from(home).join("Library").join("Application Support"));
            }
        }
        // Linux：优先 $XDG_DATA_HOME，回退到 ~/.local/share
        #[cfg(target_os = "linux")]
        {
            if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
                return Some(PathBuf::from(xdg));
            }
            if let Ok(home) = std::env::var("HOME") {
                return Some(PathBuf::from(home).join(".local").join("share"));
            }
        }
        // Windows：%APPDATA%
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return Some(PathBuf::from(appdata));
            }
        }
        None
    }
}
