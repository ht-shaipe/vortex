//! 回程 invoke 注册表：建立正向 `call_id → invoke` 映射，供回程查询工具名。
//!
//! 按 `request_id` → `call_id` 二级索引，TTL 自动清理。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::types::Invoke;

/// invoke 注册表（线程安全）。
pub struct InvokeRegistry {
    /// request_id → (call_id → Invoke)
    entries: Mutex<HashMap<String, HashMap<String, Invoke>>>,
    /// request_id → 最后更新时间
    timestamps: Mutex<HashMap<String, Instant>>,
    ttl_seconds: u64,
}

impl InvokeRegistry {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            timestamps: Mutex::new(HashMap::new()),
            ttl_seconds,
        }
    }

    /// 注册 call_id → invoke 映射。
    pub fn register(&self, request_id: &str, call_id: &str, invoke: Invoke) {
        {
            let mut entries = self.entries.lock().unwrap();
            entries
                .entry(request_id.to_string())
                .or_default()
                .insert(call_id.to_string(), invoke);
        }
        let mut timestamps = self.timestamps.lock().unwrap();
        timestamps.insert(request_id.to_string(), Instant::now());
    }

    /// 查询 call_id 对应的 invoke。
    pub fn lookup(&self, request_id: &str, call_id: &str) -> Option<Invoke> {
        let entries = self.entries.lock().unwrap();
        entries
            .get(request_id)
            .and_then(|m| m.get(call_id))
            .cloned()
    }

    /// 仅用 call_id 查询（遍历所有 request_id，回程时使用）。
    pub fn lookup_by_call_id(&self, call_id: &str) -> Option<Invoke> {
        let entries = self.entries.lock().unwrap();
        for m in entries.values() {
            if let Some(invoke) = m.get(call_id) {
                return Some(invoke.clone());
            }
        }
        None
    }

    /// 清除某 request_id 下所有条目。
    pub fn clear(&self, request_id: &str) {
        {
            let mut entries = self.entries.lock().unwrap();
            entries.remove(request_id);
        }
        let mut timestamps = self.timestamps.lock().unwrap();
        timestamps.remove(request_id);
    }

    /// 清理过期条目（超过 TTL）。
    pub fn purge_expired(&self) {
        let now = Instant::now();
        let ttl = Duration::from_secs(self.ttl_seconds);
        let expired: Vec<String> = {
            let timestamps = self.timestamps.lock().unwrap();
            timestamps
                .iter()
                .filter(|(_, t)| now.duration_since(**t) > ttl)
                .map(|(k, _)| k.clone())
                .collect()
        };
        for key in expired {
            self.clear(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_lookup_clear() {
        let reg = InvokeRegistry::new(300);
        let invoke = Invoke {
            name: "bash".into(),
            parameters: vec![],
        };
        reg.register("req1", "call_1", invoke.clone());
        assert_eq!(reg.lookup("req1", "call_1"), Some(invoke));
        reg.clear("req1");
        assert_eq!(reg.lookup("req1", "call_1"), None);
    }

    #[test]
    fn test_concurrent_safety() {
        let reg = std::sync::Arc::new(InvokeRegistry::new(300));
        let mut handles = vec![];
        for i in 0..10 {
            let r = reg.clone();
            handles.push(std::thread::spawn(move || {
                r.register(
                    &format!("req{i}"),
                    "call_x",
                    Invoke {
                        name: "bash".into(),
                        parameters: vec![],
                    },
                );
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        for i in 0..10 {
            assert!(reg.lookup(&format!("req{i}"), "call_x").is_some());
        }
    }
}