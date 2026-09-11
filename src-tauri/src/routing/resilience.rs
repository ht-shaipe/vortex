//! 熔断器与弹性管理
//!
//! 实现三态熔断器（Closed / Open / HalfOpen）对上游连接进行故障隔离。
//! 当某连接连续失败达到阈值时熔断打开，拒绝请求；经过冷却时间后进入半开态，
//! 放行少量探测请求，探测成功则恢复，失败则重新熔断。

use std::time::{Duration, Instant};

/// 熔断器状态
///
/// 采用经典三态模型：
/// - `Closed`：正常放行，累计失败次数
/// - `Open`：熔断打开，拒绝所有请求
/// - `HalfOpen`：半开探测，放行请求但观察结果
#[derive(Debug, Clone, PartialEq)]
pub enum BreakerState {
    /// 关闭态：正常工作，累计失败计数
    Closed,
    /// 打开态：熔断中，拒绝请求
    Open,
    /// 半开态：探测中，放行并观察结果
    HalfOpen,
}

/// 熔断器
///
/// 对单个连接进行故障隔离。失败次数达到阈值后进入 Open 态，
/// 经过冷却时间后自动转为 HalfOpen 态进行探测。
pub struct CircuitBreaker {
    /// 熔断器名称（通常为 "provider_id:connection_id"）
    name: String,
    /// 当前熔断状态
    state: parking_lot::Mutex<BreakerState>,
    /// 连续失败计数
    failure_count: parking_lot::Mutex<u32>,
    /// 最近一次失败时间
    last_failure: parking_lot::Mutex<Option<Instant>>,
    /// 失败次数阈值，达到后熔断打开
    threshold: u32,
    /// 熔断冷却时长
    reset_duration: Duration,
    /// 半开态下的成功计数
    success_count_in_halfopen: parking_lot::Mutex<u32>,
}

impl CircuitBreaker {
    /// 创建熔断器
    ///
    /// # 参数
    /// - `name`：熔断器名称
    /// - `threshold`：失败次数阈值
    /// - `reset_duration_secs`：冷却时长（秒）
    pub fn new(name: &str, threshold: u32, reset_duration_secs: u64) -> Self {
        Self {
            name: name.to_string(),
            state: parking_lot::Mutex::new(BreakerState::Closed),
            failure_count: parking_lot::Mutex::new(0),
            last_failure: parking_lot::Mutex::new(None),
            threshold,
            reset_duration: Duration::from_secs(reset_duration_secs),
            success_count_in_halfopen: parking_lot::Mutex::new(0),
        }
    }

    /// 判断当前是否允许请求通过
    ///
    /// - `Closed`：始终放行
    /// - `Open`：冷却时间已过则转为 `HalfOpen` 并放行，否则拒绝
    /// - `HalfOpen`：放行
    pub fn is_available(&self) -> bool {
        let state = self.state.lock();
        match *state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                let last = self.last_failure.lock();
                if let Some(t) = *last {
                    // 冷却时间已过，转入半开态
                    if t.elapsed() >= self.reset_duration {
                        drop(state);
                        *self.state.lock() = BreakerState::HalfOpen;
                        *self.success_count_in_halfopen.lock() = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            BreakerState::HalfOpen => true,
        }
    }

    /// 记录一次成功
    ///
    /// - `HalfOpen`：成功计数 +1，达到 3 次则恢复为 `Closed`
    /// - 其他状态：直接恢复为 `Closed` 并清零失败计数
    pub fn record_success(&self) {
        let mut state = self.state.lock();
        match *state {
            BreakerState::HalfOpen => {
                let mut count = self.success_count_in_halfopen.lock();
                *count += 1;
                // 半开态连续成功 3 次后完全恢复
                if *count >= 3 {
                    *state = BreakerState::Closed;
                    *self.failure_count.lock() = 0;
                }
            }
            _ => {
                *state = BreakerState::Closed;
                *self.failure_count.lock() = 0;
            }
        }
    }

    /// 记录一次失败
    ///
    /// 失败计数 +1，若达到阈值则转为 `Open`；
    /// 若当前为 `HalfOpen` 则直接回到 `Open`。
    pub fn record_failure(&self) {
        let mut count = self.failure_count.lock();
        *count += 1;
        *self.last_failure.lock() = Some(Instant::now());

        // 失败次数达到阈值，熔断打开
        if *count >= self.threshold {
            *self.state.lock() = BreakerState::Open;
        } else if *self.state.lock() == BreakerState::HalfOpen {
            // 半开态失败，立即回到打开态
            *self.state.lock() = BreakerState::Open;
        }
    }
}

/// 弹性管理器
///
/// 管理多个熔断器实例，按名称索引。未注册的名称视为始终可用。
pub struct ResilienceManager {
    /// 熔断器列表
    breakers: parking_lot::Mutex<Vec<CircuitBreaker>>,
}

impl ResilienceManager {
    /// 创建弹性管理器
    pub fn new() -> Self {
        Self {
            breakers: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// 判断指定名称的连接是否可用
    ///
    /// 未注册的名称视为始终可用（首次请求时无熔断器记录）。
    pub fn is_available(&self, name: &str) -> bool {
        let breakers = self.breakers.lock();
        if let Some(b) = breakers.iter().find(|b| b.name == name) {
            b.is_available()
        } else {
            true
        }
    }

    /// 记录指定名称连接的一次成功
    pub fn record_success(&self, name: &str) {
        let breakers = self.breakers.lock();
        if let Some(b) = breakers.iter().find(|b| b.name == name) {
            b.record_success();
        }
    }

    /// 记录指定名称连接的一次失败
    ///
    /// 若该名称尚无熔断器，则自动创建一个（阈值 5，冷却 60 秒）并记录失败。
    pub fn record_failure(&self, name: &str) {
        let mut breakers = self.breakers.lock();
        if let Some(b) = breakers.iter_mut().find(|b| b.name == name) {
            b.record_failure();
            return;
        }
        // 首次失败：创建新熔断器（阈值 5 次，冷却 60 秒）
        drop(breakers);
        let new_breaker = CircuitBreaker::new(name, 5, 60);
        new_breaker.record_failure();
        self.breakers.lock().push(new_breaker);
    }

    /// 重置指定名称连接的熔断器为关闭态。
    ///
    /// 用于虚拟模型映射故障转移：当目标连接的熔断器处于打开态时，
    /// 用户主动发起请求说明希望尝试该连接，重置后给予一次机会。
    pub fn reset(&self, name: &str) {
        let breakers = self.breakers.lock();
        if let Some(b) = breakers.iter().find(|b| b.name == name) {
            *b.state.lock() = BreakerState::Closed;
            *b.failure_count.lock() = 0;
            *b.last_failure.lock() = None;
        }
    }
}
