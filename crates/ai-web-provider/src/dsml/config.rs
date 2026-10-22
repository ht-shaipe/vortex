//! DSML 配置：从环境变量加载，缺省项使用默认值。

use regex::Regex;

/// DSML 全局配置。
#[derive(Debug, Clone)]
pub struct DsmlConfig {
    /// 双写模式：content 保留 DSML 原文 + tool_calls 并存
    pub dual_write_mode: bool,
    /// 未知标签保留模式：遇到非 DSML 标签时原样保留而非降级
    pub unknown_tag_passthrough: bool,
    /// 流式缓冲上限（字节），超限触发降级
    pub buffer_limit: usize,
    /// 单次请求解析内存上限（字节）
    pub memory_limit: usize,
    /// id 冲突重试上限
    pub id_retry_limit: u8,
    /// 脱敏正则规则
    pub desensitize_rules: Vec<Regex>,
    /// 回程最大轮次
    pub max_return_trip_rounds: u32,
    /// InvokeRegistry TTL（秒）
    pub invoke_registry_ttl_seconds: u64,
    /// 网页注入后等待 DOM 稳定时间（毫秒）
    pub web_inject_settle_ms: u64,
    /// 回程文本格式：natural_language / markdown / json
    pub tool_result_format: String,
}

impl Default for DsmlConfig {
    fn default() -> Self {
        let desensitize_rules = vec![
            // 长度 ≥16 的连续 Base64 串
            Regex::new(r"[A-Za-z0-9+/]{16,}").unwrap(),
            // 长度 ≥16 的连续 Hex 串
            Regex::new(r"[0-9a-fA-F]{16,}").unwrap(),
            // AKIA 开头的 AWS 密钥
            Regex::new(r"AKIA[A-Za-z0-9]*").unwrap(),
            // SK 开头的密钥串
            Regex::new(r"SK[A-Za-z0-9]{10,}").unwrap(),
        ];
        Self {
            dual_write_mode: false,
            unknown_tag_passthrough: true,
            buffer_limit: 65536,
            memory_limit: 16777216,
            id_retry_limit: 5,
            desensitize_rules,
            max_return_trip_rounds: 10,
            invoke_registry_ttl_seconds: 300,
            web_inject_settle_ms: 1000,
            tool_result_format: "natural_language".to_string(),
        }
    }
}

impl DsmlConfig {
    /// 从环境变量加载配置，缺省项使用默认值。
    pub fn load() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("DSML_DUAL_WRITE_MODE") {
            cfg.dual_write_mode = v == "true" || v == "1";
        }
        if let Ok(v) = std::env::var("DSML_UNKNOWN_TAG_PASSTHROUGH") {
            cfg.unknown_tag_passthrough = v == "true" || v == "1";
        }
        if let Ok(v) = std::env::var("DSML_BUFFER_LIMIT") {
            if let Ok(n) = v.parse() {
                cfg.buffer_limit = n;
            }
        }
        if let Ok(v) = std::env::var("DSML_MEMORY_LIMIT") {
            if let Ok(n) = v.parse() {
                cfg.memory_limit = n;
            }
        }
        if let Ok(v) = std::env::var("DSML_ID_RETRY_LIMIT") {
            if let Ok(n) = v.parse() {
                cfg.id_retry_limit = n;
            }
        }
        if let Ok(v) = std::env::var("DSML_MAX_RETURN_TRIP_ROUNDS") {
            if let Ok(n) = v.parse() {
                cfg.max_return_trip_rounds = n;
            }
        }
        if let Ok(v) = std::env::var("DSML_INVOKE_REGISTRY_TTL_SECONDS") {
            if let Ok(n) = v.parse() {
                cfg.invoke_registry_ttl_seconds = n;
            }
        }
        if let Ok(v) = std::env::var("DSML_WEB_INJECT_SETTLE_MS") {
            if let Ok(n) = v.parse() {
                cfg.web_inject_settle_ms = n;
            }
        }
        if let Ok(v) = std::env::var("DSML_TOOL_RESULT_FORMAT") {
            cfg.tool_result_format = v;
        }
        cfg
    }
}