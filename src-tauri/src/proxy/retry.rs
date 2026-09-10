use std::time::Duration;

pub struct RetryPolicy {
    max_retries: u32,
    base_delay_ms: u64,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, base_delay_ms: u64) -> Self {
        Self { max_retries, base_delay_ms }
    }

    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.base_delay_ms * 2u64.pow(attempt);
        Duration::from_millis(delay.min(30_000))
    }

    pub fn should_retry(&self, attempt: u32, status_code: Option<u16>) -> bool {
        if attempt >= self.max_retries {
            return false;
        }
        if let Some(code) = status_code {
            code >= 500 || code == 429
        } else {
            true
        }
    }
}
