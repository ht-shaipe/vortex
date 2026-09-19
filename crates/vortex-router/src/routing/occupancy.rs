//! 模型占用追踪器
//!
//! 记录"哪个智能体最近正在使用哪个模型"，供 auto 路由做软避让：
//! 当 Agent A 正在使用某模型时，Agent B 的 auto 请求尽量不选同一模型，
//! 避免争抢上游限流配额；若所有候选都被占用则仍然可用（不拒绝服务）。
//!
//! 采用"最近活跃时间窗"而非精确的 in-flight 计数：无需追踪流式响应的
//! 生命周期，请求开始/结束时刷新时间戳即可，过期自动失效。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 默认活跃窗口：60 秒内的使用视为"正在占用"
const DEFAULT_TTL_SECS: u64 = 60;

/// 模型占用追踪器
///
/// 线程安全：内部使用 parking_lot::Mutex 保护，条目规模为
/// 模型数 × 智能体数（通常 < 100），无性能顾虑。
pub struct OccupancyTracker {
    /// (模型键, 智能体标识) -> 最近活跃时刻
    active: parking_lot::Mutex<HashMap<(String, String), Instant>>,
    /// 活跃窗口时长
    ttl: Duration,
}

impl OccupancyTracker {
    /// 创建追踪器（默认 60 秒活跃窗口）
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(DEFAULT_TTL_SECS))
    }

    /// 以指定窗口时长创建追踪器
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            active: parking_lot::Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// 记录某智能体正在使用某模型（刷新活跃时间戳）
    ///
    /// 顺带清理过期条目，避免长期运行下的内存缓慢增长。
    pub fn touch(&self, model_key: &str, agent: &str) {
        let now = Instant::now();
        let mut map = self.active.lock();
        // 过期清理：条目极少，遍历成本可忽略
        map.retain(|_, ts| now.duration_since(*ts) < self.ttl);
        map.insert((model_key.to_string(), agent.to_string()), now);
    }

    /// 判断某模型是否正被"其他智能体"占用
    ///
    /// - `agent` 为 `Some(自身标识)`：存在 TTL 内其他智能体的活跃记录即视为占用
    /// - `agent` 为 `None`（未识别来源）：不做避让判定，返回 false
    pub fn busy_by_others(&self, model_key: &str, agent: Option<&str>) -> bool {
        let Some(me) = agent else { return false };
        let now = Instant::now();
        let map = self.active.lock();
        map.iter().any(|((key, holder), ts)| {
            key == model_key && holder != me && now.duration_since(*ts) < self.ttl
        })
    }
}

impl Default for OccupancyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_then_busy_for_others() {
        let t = OccupancyTracker::new();
        t.touch("openai:gpt-x:c1", "claude_code");
        // 其他智能体视角：被占用
        assert!(t.busy_by_others("openai:gpt-x:c1", Some("codex")));
        // 自己视角：不算占用
        assert!(!t.busy_by_others("openai:gpt-x:c1", Some("claude_code")));
        // 未识别来源：不避让
        assert!(!t.busy_by_others("openai:gpt-x:c1", None));
        // 无关模型：不占用
        assert!(!t.busy_by_others("openai:gpt-y:c1", Some("codex")));
    }

    #[test]
    fn expired_entries_are_not_busy() {
        let t = OccupancyTracker::with_ttl(Duration::from_millis(10));
        t.touch("m", "a");
        std::thread::sleep(Duration::from_millis(30));
        assert!(!t.busy_by_others("m", Some("b")));
    }

    #[test]
    fn touch_prunes_expired_entries() {
        let t = OccupancyTracker::with_ttl(Duration::from_millis(10));
        t.touch("m1", "a");
        std::thread::sleep(Duration::from_millis(30));
        t.touch("m2", "a");
        let map = t.active.lock();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&("m2".to_string(), "a".to_string())));
    }
}
