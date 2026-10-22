//! DSML 指标暴露：轻量 AtomicU64 计数器，通过 tracing 日志输出。
//!
//! 不引入 prometheus 重依赖；如需接入 Prometheus，可在 `/metrics` 端点
//! 读取这些计数器序列化输出。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::types::DegradeReason;

/// DSML 指标集合（线程安全，可 Arc 共享）。
#[derive(Debug)]
pub struct DsmlMetrics {
    pub parse_ok_count: AtomicU64,
    pub degrade_count: AtomicU64,
    pub parse_latency_ms_total: AtomicU64,
    pub parse_latency_count: AtomicU64,
    pub buffer_peak_bytes: AtomicU64,
    pub return_trip_rounds_total: AtomicU64,
    pub return_trip_count: AtomicU64,
    pub registry_miss_count: AtomicU64,
}

impl DsmlMetrics {
    /// 创建指标实例（等价于 register，返回 Arc 便于共享）。
    pub fn register() -> Arc<Self> {
        Arc::new(Self {
            parse_ok_count: AtomicU64::new(0),
            degrade_count: AtomicU64::new(0),
            parse_latency_ms_total: AtomicU64::new(0),
            parse_latency_count: AtomicU64::new(0),
            buffer_peak_bytes: AtomicU64::new(0),
            return_trip_rounds_total: AtomicU64::new(0),
            return_trip_count: AtomicU64::new(0),
            registry_miss_count: AtomicU64::new(0),
        })
    }

    /// 记录一次成功解析
    pub fn record_parse_ok(&self, latency_ms: u64) {
        self.parse_ok_count.fetch_add(1, Ordering::Relaxed);
        self.parse_latency_ms_total
            .fetch_add(latency_ms, Ordering::Relaxed);
        self.parse_latency_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次降级
    pub fn record_degrade(&self, _reason: DegradeReason) {
        self.degrade_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 设置缓冲峰值
    pub fn set_buffer_peak(&self, bytes: u64) {
        self.buffer_peak_bytes.fetch_max(bytes, Ordering::Relaxed);
    }

    /// 记录一次回程编排
    pub fn record_return_trip(&self, rounds: u32) {
        self.return_trip_rounds_total
            .fetch_add(rounds as u64, Ordering::Relaxed);
        self.return_trip_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次 registry 未命中
    pub fn record_registry_miss(&self) {
        self.registry_miss_count.fetch_add(1, Ordering::Relaxed);
    }
}