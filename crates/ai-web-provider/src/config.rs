//! 配置：环境变量解析 + 全平台共享配置（CommonCfg）+ 单平台配置（ProviderCfg）

use std::path::PathBuf;

/// 展开 ~/ 前缀
pub fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

/// 全平台共享配置（来自环境变量）
#[derive(Clone)]
pub struct CommonCfg {
    /// obscura 存储根目录（各平台子目录 = {base}/{platform}）
    pub storage_base: String,
    pub max_wait_ms: u64,
    pub stable_ms: u64,
    /// 调试模式：开启后每步操作渲染完把页面 HTML 存到项目 html/ 目录
    pub debug_html: bool,
}

impl CommonCfg {
    pub fn from_env() -> Self {
        let storage_base = expand_home(
            &std::env::var("STORAGE_DIR").unwrap_or_else(|_| "~/.obscura".into()),
        );
        let max_wait_ms: u64 = std::env::var("MAX_WAIT_MS")
            .unwrap_or_else(|_| "120000".into())
            .parse()
            .unwrap_or(120000);
        let stable_ms: u64 = std::env::var("STABLE_MS")
            .unwrap_or_else(|_| "5000".into())
            .parse()
            .unwrap_or(5000);
        // 调试模式（默认开启）：每步操作渲染完把页面 HTML 存到项目 html/ 目录；
        // 设 DEBUG_HTML=0/false 可关闭（也可通过 POST /api/{platform}/debug/html 运行时切换）
        let debug_html = std::env::var("DEBUG_HTML")
            .map(|v| !(v == "0" || v.is_empty() || v.to_lowercase() == "false"))
            .unwrap_or(true);
        Self {
            storage_base,
            max_wait_ms,
            stable_ms,
            debug_html,
        }
    }

    /// 解析 PLATFORMS 环境变量（逗号分隔，默认全平台）
    pub fn platform_list() -> Vec<String> {
        let raw = std::env::var("PLATFORMS")
            .unwrap_or_else(|_| "chatgpt,claude,doubao,deepseek,qianwen,kimi".into());
        raw.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// 单平台配置
#[derive(Clone)]
pub struct ProviderCfg {
    /// 平台标识（小写，用于 URL 路径与存储子目录名）：doubao / deepseek / qianwen / kimi
    pub name: String,
    /// 平台显示名：豆包 / DeepSeek / 通义千问 / Kimi（供日志与后续平台接入使用）
    #[allow(dead_code)]
    pub display: String,
    /// 站点地址（可被 {平台}_URL 环境变量覆盖）
    pub base_url: String,
    /// obscura 存储目录（{storage_base}/{name}）
    pub storage_dir: String,
    /// cookie 持久化文件（storage_dir/cookies.json）。
    /// obscura 启动时自动从该文件恢复 cookie；Some 时登录成功后与心跳
    /// 周期把 cookie jar 落盘，重启无需重新扫码。
    pub cookie_path: Option<PathBuf>,
    pub max_wait_ms: u64,
    pub stable_ms: u64,
    pub debug_html: bool,
}

impl ProviderCfg {
    /// 按平台构造配置
    pub fn for_platform(
        common: &CommonCfg,
        name: &str,
        display: &str,
        default_url: &str,
        url_env: &str,
    ) -> Self {
        let base_url =
            std::env::var(url_env).unwrap_or_else(|_| default_url.to_string());
        let storage_dir =
            format!("{}/{}", common.storage_base.trim_end_matches('/'), name);
        let cookie_path = PathBuf::from(&storage_dir).join("cookies.json");
        Self {
            name: name.to_string(),
            display: display.to_string(),
            base_url,
            storage_dir,
            cookie_path: Some(cookie_path),
            max_wait_ms: common.max_wait_ms,
            stable_ms: common.stable_ms,
            debug_html: common.debug_html,
        }
    }
}
