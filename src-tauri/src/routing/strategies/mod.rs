use crate::db::models::{ResolvedComboTarget, RoutingContext};
use crate::error::Result;
use rand::Rng;
use std::sync::atomic::{AtomicUsize, Ordering};

pub trait RoutingStrategy: Send + Sync {
    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>>;
}

pub struct Priority;

impl RoutingStrategy for Priority {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        Ok(targets.iter().collect())
    }
}

pub struct FillFirst {
    current_target: AtomicUsize,
    fill_count: parking_lot::Mutex<Vec<u64>>,
}

impl FillFirst {
    pub fn new() -> Self {
        Self {
            current_target: AtomicUsize::new(0),
            fill_count: parking_lot::Mutex::new(Vec::new()),
        }
    }

    fn ensure_counts(&self, len: usize) {
        let mut counts = self.fill_count.lock();
        while counts.len() < len {
            counts.push(0);
        }
    }
}

impl RoutingStrategy for FillFirst {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        self.ensure_counts(targets.len());
        let mut counts = self.fill_count.lock();
        let current = self.current_target.load(Ordering::Relaxed) % targets.len();

        let max_fill = targets[current].weight as u64;
        if max_fill > 0 && counts[current] >= max_fill {
            counts[current] = 0;
            let next = (current + 1) % targets.len();
            self.current_target.store(next, Ordering::Relaxed);
        }

        let idx = self.current_target.load(Ordering::Relaxed) % targets.len();
        counts[idx] += 1;

        let mut result = vec![&targets[idx]];
        for (i, target) in targets.iter().enumerate() {
            if i != idx {
                result.push(target);
            }
        }
        Ok(result)
    }
}

pub struct Weighted;

impl RoutingStrategy for Weighted {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let total_weight: f64 = targets.iter().map(|t| t.weight.max(0.01)).sum();
        let mut rng = rand::thread_rng();

        let primary_idx = {
            let mut found = 0usize;
            for _ in 0..100 {
                let mut roll = rng.r#gen::<f64>() * total_weight;
                for (i, target) in targets.iter().enumerate() {
                    roll -= target.weight.max(0.01);
                    if roll <= 0.0 {
                        found = i;
                        break;
                    }
                }
            }
            found
        };

        let mut result = vec![&targets[primary_idx]];
        for target in targets {
            if target.step_id != result[0].step_id {
                result.push(target);
            }
        }
        Ok(result)
    }
}

pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self { counter: AtomicUsize::new(0) }
    }
}

impl RoutingStrategy for RoundRobin {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % targets.len();
        let mut result = Vec::new();
        for i in 0..targets.len() {
            result.push(&targets[(idx + i) % targets.len()]);
        }
        Ok(result)
    }
}

pub struct P2C;

impl RoutingStrategy for P2C {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.len() <= 2 {
            return Ok(targets.iter().collect());
        }
        let mut rng = rand::thread_rng();
        let i = rng.gen_range(0..targets.len());
        let j = loop {
            let j = rng.gen_range(0..targets.len());
            if j != i { break j; }
        };

        let primary = if targets[i].weight >= targets[j].weight { &targets[i] } else { &targets[j] };
        let mut result = vec![primary];
        for target in targets {
            if target.step_id != result[0].step_id {
                result.push(target);
            }
        }
        Ok(result)
    }
}

pub struct LeastUsed {
    usage_counts: parking_lot::Mutex<Vec<u64>>,
}

impl LeastUsed {
    pub fn new() -> Self {
        Self { usage_counts: parking_lot::Mutex::new(Vec::new()) }
    }

    fn ensure_counts(&self, len: usize) {
        let mut counts = self.usage_counts.lock();
        while counts.len() < len {
            counts.push(0);
        }
    }
}

impl RoutingStrategy for LeastUsed {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        self.ensure_counts(targets.len());
        let mut counts = self.usage_counts.lock();

        let mut indexed: Vec<(usize, u64)> = counts.iter().enumerate().map(|(i, &c)| (i, c)).collect();
        indexed.sort_by_key(|&(_, c)| c);

        indexed[0].1 += 1;
        *counts = vec![0u64; targets.len()];
        for (idx, count) in &indexed {
            counts[*idx] = *count;
        }

        let mut result = Vec::new();
        for (idx, _) in indexed {
            if idx < targets.len() {
                result.push(&targets[idx]);
            }
        }
        Ok(result)
    }
}

pub struct Random;

impl RoutingStrategy for Random {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..targets.len());
        let mut result = vec![&targets[idx]];
        for target in targets {
            if target.step_id != result[0].step_id {
                result.push(target);
            }
        }
        Ok(result)
    }
}

pub struct StrictRandom;

impl RoutingStrategy for StrictRandom {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        let mut indices: Vec<usize> = (0..targets.len()).collect();
        let mut rng = rand::thread_rng();
        let mut result = Vec::new();
        while !indices.is_empty() {
            let i = rng.gen_range(0..indices.len());
            result.push(&targets[indices[i]]);
            indices.remove(i);
        }
        Ok(result)
    }
}

pub struct CostOptimized;

impl RoutingStrategy for CostOptimized {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let cost_data = &context.cost_catalog;
        let mut scored: Vec<(f64, &ResolvedComboTarget)> = targets.iter().map(|t| {
            let cost = cost_data.get(&t.provider_id)
                .and_then(|p| p.get(&t.model_str))
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            (cost, t)
        }).collect();

        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct Headroom;

impl RoutingStrategy for Headroom {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let quota = &context.quota_state;
        let mut scored: Vec<(f64, &ResolvedComboTarget)> = targets.iter().map(|t| {
            let remaining = quota.get(&t.provider_id)
                .and_then(|p| p.get("remaining_pct"))
                .and_then(|v| v.as_f64())
                .unwrap_or(100.0);
            (remaining, t)
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct ResetWindow;

impl RoutingStrategy for ResetWindow {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let quota = &context.quota_state;
        let mut scored: Vec<(i64, &ResolvedComboTarget)> = targets.iter().map(|t| {
            let reset_at = quota.get(&t.provider_id)
                .and_then(|p| p.get("next_reset_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp())
                .unwrap_or(i64::MAX);
            (reset_at, t)
        }).collect();

        scored.sort_by_key(|(ts, _)| *ts);
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct ResetAware;

impl RoutingStrategy for ResetAware {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let quota = &context.quota_state;
        let now_ts = chrono::Utc::now().timestamp();

        let mut scored: Vec<(f64, &ResolvedComboTarget)> = targets.iter().map(|t| {
            let remaining = quota.get(&t.provider_id)
                .and_then(|p| p.get("remaining_pct"))
                .and_then(|v| v.as_f64())
                .unwrap_or(100.0);
            let reset_at = quota.get(&t.provider_id)
                .and_then(|p| p.get("next_reset_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp())
                .unwrap_or(now_ts + 3600);
            let window_secs = (reset_at - now_ts).max(0);
            let score = remaining * (1.0 / (1.0 + window_secs as f64 / 60.0));
            (score, t)
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct ContextRelay;

impl RoutingStrategy for ContextRelay {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        let mut scored: Vec<(f64, &ResolvedComboTarget)> = targets.iter().enumerate().map(|(_i, t)| {
            let model_lower = t.model_str.to_lowercase();
            let ctx_bonus = if model_lower.contains("128k") || model_lower.contains("200k") {
                100.0
            } else if model_lower.contains("32k") {
                60.0
            } else if model_lower.contains("16k") {
                40.0
            } else {
                20.0
            };
            let cost_penalty = if model_lower.contains("mini") || model_lower.contains("flash") || model_lower.contains("haiku") {
                0.0
            } else {
                10.0
            };
            (ctx_bonus - cost_penalty + t.weight * 5.0, t)
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct ContextOptimized;

impl RoutingStrategy for ContextOptimized {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let request_tokens = context.request_tokens.unwrap_or(0) as f64;

        let mut scored: Vec<(f64, &ResolvedComboTarget)> = targets.iter().map(|t| {
            let model_lower = t.model_str.to_lowercase();
            let model_ctx = if model_lower.contains("128k") {
                128000.0
            } else if model_lower.contains("200k") {
                200000.0
            } else if model_lower.contains("32k") {
                32000.0
            } else if model_lower.contains("16k") {
                16000.0
            } else if model_lower.contains("8k") {
                8000.0
            } else {
                4000.0
            };

            let headroom = model_ctx - request_tokens;
            let fit_score = if headroom < 0.0 {
                -1000.0
            } else {
                1.0 / (1.0 + (headroom / model_ctx).abs())
            };

            let cost_factor = if model_lower.contains("mini") || model_lower.contains("flash") || model_lower.contains("haiku") {
                10.0
            } else {
                0.0
            };

            (fit_score * 100.0 + cost_factor + t.weight * 5.0, t)
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct LKGP {
    last_good: parking_lot::Mutex<Option<String>>,
}

impl LKGP {
    pub fn new() -> Self {
        Self { last_good: parking_lot::Mutex::new(None) }
    }
}

impl RoutingStrategy for LKGP {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        let last_good = match _context.last_good_target {
            Some(ref v) => Some(v.clone()),
            None => self.last_good.lock().clone(),
        };
        if let Some(ref last) = last_good {
            if let Some(primary) = targets.iter().find(|t| t.step_id == *last || t.model_str == *last) {
                *self.last_good.lock() = Some(primary.step_id.clone());
                let mut result = vec![primary];
                for target in targets {
                    if target.step_id != result[0].step_id {
                        result.push(target);
                    }
                }
                return Ok(result);
            }
        }
        let primary = &targets[0];
        *self.last_good.lock() = Some(primary.step_id.clone());
        Ok(targets.iter().collect())
    }
}

pub struct Auto;

impl RoutingStrategy for Auto {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        let now_ts = chrono::Utc::now().timestamp();

        let mut scored: Vec<(f64, &ResolvedComboTarget)> = targets.iter().map(|t| {
            let mut score = 0.0;
            score += t.weight * 20.0;
            if context.last_good_target.as_deref() == Some(&t.step_id) || context.last_good_target.as_deref() == Some(&t.model_str) {
                score += 30.0;
            }
            let remaining = context.quota_state.get(&t.provider_id)
                .and_then(|p| p.get("remaining_pct"))
                .and_then(|v| v.as_f64())
                .unwrap_or(80.0);
            score += remaining * 0.3;
            let reset_at = context.quota_state.get(&t.provider_id)
                .and_then(|p| p.get("next_reset_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp())
                .unwrap_or(now_ts + 3600);
            let window_secs = (reset_at - now_ts).max(0);
            if window_secs < 300 {
                score += 15.0;
            }
            let cost = context.cost_catalog.get(&t.provider_id)
                .and_then(|p| p.get(&t.model_str))
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            score -= cost * 10.0;
            let latency = context.latency_history.get(&t.provider_id)
                .and_then(|p| p.get("avg_ms"))
                .and_then(|v| v.as_f64())
                .unwrap_or(500.0);
            score -= latency / 100.0;
            if t.weight > 0.0 {
                score += 10.0;
            }
            score += rand::thread_rng().r#gen::<f64>() * 5.0;
            (score, t)
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(_, t)| t).collect())
    }
}

pub struct Fusion;

impl RoutingStrategy for Fusion {

    fn select<'a>(
        &self,
        targets: &'a [ResolvedComboTarget],
        _context: &RoutingContext,
    ) -> Result<Vec<&'a ResolvedComboTarget>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        Ok(targets.iter().collect())
    }
}
