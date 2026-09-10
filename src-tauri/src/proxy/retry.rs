//! 重试策略
//!
//! 提供指数退避重试机制：最大重试次数与基础延迟可配置，
//! 延迟按 `base_delay * 2^attempt` 指数增长，上限 30 秒。
//! 仅对 5xx 服务端错误和 429 限流状态码重试。

use std::time::Duration;

/// 重试策略配置
///
/// 控制请求失败后的重试行为，采用指数退避算法。
pub struct RetryPolicy {
    /// 最大重试次数（不包含首次请求）
    max_retries: u32,
    /// 基础延迟（毫秒），实际延迟为 `base_delay_ms * 2^attempt`
    base_delay_ms: u64,
}

impl RetryPolicy {
    /// 创建重试策略
    ///
    /// # 参数
    /// - `max_retries`：最大重试次数
    /// - `base_delay_ms`：基础延迟毫秒数
    pub fn new(max_retries: u32, base_delay_ms: u64) -> Self {
        Self { max_retries, base_delay_ms }
    }

    /// 返回最大重试次数
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// 计算指定尝试次数对应的退避延迟
    ///
    /// 延迟 = `base_delay_ms * 2^attempt`，上限 30 秒。
    ///
    /// # 参数
    /// - `attempt`：当前尝试序号（0 表示首次失败后的第一次重试）
    ///
    /// # 返回
    /// 对应的等待时长
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        // 指数退避：基础延迟乘以 2 的 attempt 次方
        let delay = self.base_delay_ms * 2u64.pow(attempt);
        // 上限 30 秒，防止延迟过大
        Duration::from_millis(delay.min(30_000))
    }

    /// 判断是否应进行重试
    ///
    /// # 参数
    /// - `attempt`：当前尝试序号
    /// - `status_code`：上游 HTTP 状态码（网络错误时为 `None`）
    ///
    /// # 返回
    /// `true` 表示应重试。仅对 5xx 和 429 重试；网络错误（`None`）始终重试。
    pub fn should_retry(&self, attempt: u32, status_code: Option<u16>) -> bool {
        // 已达最大重试次数，不再重试
        if attempt >= self.max_retries {
            return false;
        }
        if let Some(code) = status_code {
            // 仅对服务端错误和限流重试
            code >= 500 || code == 429
        } else {
            // 网络错误始终重试
            true
        }
    }
}
