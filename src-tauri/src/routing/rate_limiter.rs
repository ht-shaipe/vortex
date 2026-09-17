//! 内存速率限制器
//!
//! 在路由层做快速预检查，避免每次请求都查数据库。
//! 后台定期从 DB 同新限额配置和清理过期计数。

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 单个限速条目的内存计数
#[derive(Clone)]
struct RateCounter {
    /// 每分钟请求时间戳列表（滑动窗口）
    rpm_window: Vec<Instant>,
    /// 每天请求时间戳列表（滑动窗口）
    rpd_window: Vec<Instant>,
    /// 每分钟 token 累计（与请求时间戳对齐）
    tpm_window: Vec<(Instant, i64)>,
    /// 每天 token 累计（与请求时间戳对齐）
    tpd_window: Vec<(Instant, i64)>,
    /// 配置的限额
    rpm_cap: Option<i64>,
    rpd_cap: Option<i64>,
    tpm_cap: Option<i64>,
    tpd_cap: Option<i64>,
}

impl RateCounter {
    fn new() -> Self {
        Self {
            rpm_window: Vec::new(),
            rpd_window: Vec::new(),
            tpm_window: Vec::new(),
            tpd_window: Vec::new(),
            rpm_cap: None,
            rpd_cap: None,
            tpm_cap: None,
            tpd_cap: None,
        }
    }

    /// 清理过期窗口数据
    fn gc(&mut self) {
        let now = Instant::now();
        let one_min = Duration::from_secs(60);
        let one_day = Duration::from_secs(86400);
        self.rpm_window.retain(|t| now.duration_since(*t) < one_min);
        self.rpd_window.retain(|t| now.duration_since(*t) < one_day);
        self.tpm_window.retain(|(t, _)| now.duration_since(*t) < one_min);
        self.tpd_window.retain(|(t, _)| now.duration_since(*t) < one_day);
    }

    /// 检查是否允许请求通过
    fn check(&self, estimated_tokens: i64) -> bool {
        if let Some(cap) = self.rpm_cap {
            if self.rpm_window.len() as i64 >= cap {
                return false;
            }
        }
        if let Some(cap) = self.rpd_cap {
            if self.rpd_window.len() as i64 >= cap {
                return false;
            }
        }
        if let Some(cap) = self.tpm_cap {
            let current: i64 = self.tpm_window.iter().map(|(_, t)| t).sum();
            if current + estimated_tokens > cap {
                return false;
            }
        }
        if let Some(cap) = self.tpd_cap {
            let current: i64 = self.tpd_window.iter().map(|(_, t)| t).sum();
            if current + estimated_tokens > cap {
                return false;
            }
        }
        true
    }

    /// 记数一次请求
    fn record(&mut self, tokens: i64) {
        let now = Instant::now();
        self.rpm_window.push(now);
        self.rpd_window.push(now);
        self.tpm_window.push((now, tokens));
        self.tpd_window.push((now, tokens));
    }
}

/// 速率限制器
#[derive(Clone)]
pub struct RateLimiter {
    /// 按 "provider:model:connection_id" 索引的计数器
    counters: Arc<RwLock<HashMap<String, RateCounter>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 生成索引键
    fn make_key(provider: &str, model: &str, connection_id: Option<&str>) -> String {
        match connection_id {
            Some(cid) => format!("{}:{}:{}", provider, model, cid),
            None => format!("{}:{}:", provider, model),
        }
    }

    /// 检查是否允许请求通过（不记数）
    pub fn check(&self, provider: &str, model: &str, connection_id: Option<&str>, estimated_tokens: i64) -> bool {
        let key = Self::make_key(provider, model, connection_id);
        let counters = self.counters.read();
        if let Some(counter) = counters.get(&key) {
            counter.check(estimated_tokens)
        } else {
            true
        }
    }

    /// 记数一次请求
    pub fn record(&self, provider: &str, model: &str, connection_id: Option<&str>, tokens: i64) {
        let key = Self::make_key(provider, model, connection_id);
        let mut counters = self.counters.write();
        let counter = counters.entry(key).or_insert_with(RateCounter::new);
        counter.gc();
        counter.record(tokens);
    }

    /// 更新限额配置
    pub fn set_cap(
        &self,
        provider: &str,
        model: &str,
        connection_id: Option<&str>,
        rpm: Option<i64>,
        rpd: Option<i64>,
        tpm: Option<i64>,
        tpd: Option<i64>,
    ) {
        let key = Self::make_key(provider, model, connection_id);
        let mut counters = self.counters.write();
        let counter = counters.entry(key).or_insert_with(RateCounter::new);
        counter.rpm_cap = rpm;
        counter.rpd_cap = rpd;
        counter.tpm_cap = tpm;
        counter.tpd_cap = tpd;
    }

    /// 清理所有过期窗口数据
    pub fn gc_all(&self) {
        let mut counters = self.counters.write();
        for counter in counters.values_mut() {
            counter.gc();
        }
    }

    /// 获取某键的当前用量快照
    pub fn get_usage(&self, provider: &str, model: &str, connection_id: Option<&str>) -> Option<(usize, usize, i64, i64)> {
        let key = Self::make_key(provider, model, connection_id);
        let counters = self.counters.read();
        counters.get(&key).map(|c| {
            let tpm: i64 = c.tpm_window.iter().map(|(_, t)| t).sum();
            let tpd: i64 = c.tpd_window.iter().map(|(_, t)| t).sum();
            (c.rpm_window.len(), c.rpd_window.len(), tpm, tpd)
        })
    }
}
