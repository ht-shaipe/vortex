use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    name: String,
    state: parking_lot::Mutex<BreakerState>,
    failure_count: parking_lot::Mutex<u32>,
    last_failure: parking_lot::Mutex<Option<Instant>>,
    threshold: u32,
    reset_duration: Duration,
    success_count_in_halfopen: parking_lot::Mutex<u32>,
}

impl CircuitBreaker {
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

    pub fn is_available(&self) -> bool {
        let state = self.state.lock();
        match *state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                let last = self.last_failure.lock();
                if let Some(t) = *last {
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

    pub fn record_success(&self) {
        let mut state = self.state.lock();
        match *state {
            BreakerState::HalfOpen => {
                let mut count = self.success_count_in_halfopen.lock();
                *count += 1;
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

    pub fn record_failure(&self) {
        let mut count = self.failure_count.lock();
        *count += 1;
        *self.last_failure.lock() = Some(Instant::now());

        if *count >= self.threshold {
            *self.state.lock() = BreakerState::Open;
        } else if *self.state.lock() == BreakerState::HalfOpen {
            *self.state.lock() = BreakerState::Open;
        }
    }
}

pub struct ResilienceManager {
    breakers: parking_lot::Mutex<Vec<CircuitBreaker>>,
}

impl ResilienceManager {
    pub fn new() -> Self {
        Self {
            breakers: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn is_available(&self, name: &str) -> bool {
        let breakers = self.breakers.lock();
        if let Some(b) = breakers.iter().find(|b| b.name == name) {
            b.is_available()
        } else {
            true
        }
    }

    pub fn record_success(&self, name: &str) {
        let breakers = self.breakers.lock();
        if let Some(b) = breakers.iter().find(|b| b.name == name) {
            b.record_success();
        }
    }

    pub fn record_failure(&self, name: &str) {
        let mut breakers = self.breakers.lock();
        if let Some(b) = breakers.iter_mut().find(|b| b.name == name) {
            b.record_failure();
            return;
        }
        drop(breakers);
        let new_breaker = CircuitBreaker::new(name, 5, 60);
        new_breaker.record_failure();
        self.breakers.lock().push(new_breaker);
    }
}
