//! 终端（agent）粘性模型追踪器
//!
//! 为每个调用方终端（如 zcode、codex、claude-code 等 agent 指纹）记录
//! auto 路由最近一次成功的模型，实现"轮循式粘合"：
//! 同一终端的 auto 请求优先复用上次成功的模型，保持终端会话连续性；
//! 模型在重试额度内失败后由引擎清除绑定并熔断（封冻）该模型。
//!
//! 条目带 TTL（默认 24 小时），每次成功刷新，过期自动失效，
//! 避免长期运行下残留早已下线的模型绑定。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 默认粘性有效期：24 小时（每次成功刷新）
const DEFAULT_TTL_SECS: u64 = 86_400;

/// 终端粘性条目
struct StickyEntry {
    provider: String,
    model: String,
    connection_id: Option<String>,
    last_used: Instant,
}

/// 终端粘性模型追踪器
///
/// 线程安全：内部使用 Mutex 保护；条目数为终端数（通常 < 20），
/// 无性能顾虑。
pub struct AgentStickyTracker {
    /// agent 指纹 -> 最近成功绑定的模型
    entries: Mutex<HashMap<String, StickyEntry>>,
    /// 粘性有效期
    ttl: Duration,
}

impl AgentStickyTracker {
    /// 创建追踪器（默认 24 小时有效期）
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(DEFAULT_TTL_SECS))
    }

    /// 以指定有效期创建追踪器
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// 读取某终端最近成功绑定的模型（过期返回 None）
    ///
    /// 返回 `(provider, model, connection_id)`。
    pub fn get(&self, agent: &str) -> Option<(String, String, Option<String>)> {
        let entries = self.entries.lock();
        let e = entries.get(agent)?;
        if e.last_used.elapsed() >= self.ttl {
            return None;
        }
        Some((e.provider.clone(), e.model.clone(), e.connection_id.clone()))
    }

    /// 记录/刷新某终端的成功绑定
    ///
    /// 顺带清理过期条目，避免长期运行下的内存缓慢增长。
    pub fn set(&self, agent: &str, provider: &str, model: &str, connection_id: Option<&str>) {
        let mut entries = self.entries.lock();
        let ttl = self.ttl;
        entries.retain(|_, e| e.last_used.elapsed() < ttl);
        entries.insert(
            agent.to_string(),
            StickyEntry {
                provider: provider.to_string(),
                model: model.to_string(),
                connection_id: connection_id.map(|s| s.to_string()),
                last_used: Instant::now(),
            },
        );
    }

    /// 清除某终端的绑定（粘性模型连续失败、熔断打开时调用）
    pub fn clear(&self, agent: &str) {
        self.entries.lock().remove(agent);
    }
}

impl Default for AgentStickyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_get_roundtrip() {
        let t = AgentStickyTracker::new();
        assert!(t.get("zcode").is_none());
        t.set("zcode", "openai", "gpt-x", Some("c1"));
        let (p, m, c) = t.get("zcode").unwrap();
        assert_eq!(p, "openai");
        assert_eq!(m, "gpt-x");
        assert_eq!(c.as_deref(), Some("c1"));
        // 其他终端互不影响
        assert!(t.get("codex").is_none());
    }

    #[test]
    fn clear_removes_binding() {
        let t = AgentStickyTracker::new();
        t.set("zcode", "openai", "gpt-x", None);
        t.clear("zcode");
        assert!(t.get("zcode").is_none());
    }

    #[test]
    fn expired_entry_is_invalid() {
        let t = AgentStickyTracker::with_ttl(Duration::from_millis(10));
        t.set("zcode", "openai", "gpt-x", None);
        std::thread::sleep(Duration::from_millis(30));
        assert!(t.get("zcode").is_none());
    }

    #[test]
    fn set_overwrites_previous_binding() {
        let t = AgentStickyTracker::new();
        t.set("zcode", "openai", "gpt-a", Some("c1"));
        t.set("zcode", "anthropic", "claude-b", Some("c2"));
        let (p, m, _) = t.get("zcode").unwrap();
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-b");
    }
}
