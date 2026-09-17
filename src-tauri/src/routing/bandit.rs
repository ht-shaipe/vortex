//! UCB 路由评分（替代 Thompson 采样，无需额外依赖）。
//!
//! 对每个 provider+model+connection 维护成功/失败计数和延迟统计，
//! 用 UCB（Upper Confidence Bound）公式评分：score = avg_reward + exploration_bonus。
//! avg_reward 高的候选被优先选择（利用），探索次数少的候选获得探索加成（探索）。

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// 单个候选的评分状态
#[derive(Clone)]
struct ScoreArm {
    /// 成功次数
    success_count: u32,
    /// 失败次数
    failure_count: u32,
    /// 累积奖励（基于延迟的奖励总和）
    total_reward: f64,
    /// 最近一次延迟（毫秒）
    last_latency_ms: f64,
}

impl ScoreArm {
    fn new() -> Self {
        Self {
            success_count: 0,
            failure_count: 0,
            total_reward: 0.0,
            last_latency_ms: 0.0,
        }
    }

    /// 总尝试次数
    fn total(&self) -> u32 {
        self.success_count + self.failure_count
    }

    /// 平均奖励
    fn avg_reward(&self) -> f64 {
        if self.success_count == 0 {
            return 0.5;
        }
        self.total_reward / self.success_count as f64
    }

    /// 成功率
    fn success_rate(&self) -> f64 {
        let t = self.total();
        if t == 0 {
            0.5
        } else {
            self.success_count as f64 / t as f64
        }
    }

    /// UCB 评分：avg_reward + C * sqrt(ln(N) / n)
    fn ucb_score(&self, total_rounds: u32, c: f64) -> f64 {
        let exploration = if self.total() == 0 {
            1.0
        } else {
            c * (total_rounds as f64).ln().max(0.0).sqrt() / (self.total() as f64).sqrt()
        };
        self.avg_reward() * self.success_rate() + exploration
    }

    fn record_success(&mut self, latency_ms: f64) {
        self.last_latency_ms = latency_ms;
        self.success_count += 1;
        let reward = 100.0 / (100.0 + latency_ms.max(1.0));
        self.total_reward += reward;
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
    }
}

/// UCB 路由评分器
#[derive(Clone)]
pub struct BanditRouter {
    arms: Arc<RwLock<HashMap<String, ScoreArm>>>,
    /// 探索常数 C（默认 sqrt(2)），由自调优动态更新
    c: Arc<RwLock<f64>>,
    /// 总轮次
    rounds: Arc<RwLock<u32>>,
    /// auto 路由最近成功的候选 key，用于粘性优先
    last_success: Arc<RwLock<Option<String>>>,
}

impl BanditRouter {
    pub fn new() -> Self {
        Self {
            arms: Arc::new(RwLock::new(HashMap::new())),
            c: Arc::new(RwLock::new(std::f64::consts::SQRT_2)),
            rounds: Arc::new(RwLock::new(0)),
            last_success: Arc::new(RwLock::new(None)),
        }
    }

    fn make_key(provider: &str, model: &str, connection_id: Option<&str>) -> String {
        match connection_id {
            Some(cid) => format!("{}:{}:{}", provider, model, cid),
            None => format!("{}:{}:", provider, model),
        }
    }

    /// 自调优探索常数 C：每 50 轮根据整体成功率调整
    fn auto_tune(&self) {
        let rounds = *self.rounds.read();
        if rounds == 0 || rounds % 50 != 0 {
            return;
        }
        let arms = self.arms.read();
        let total_success: u32 = arms.values().map(|a| a.success_count).sum();
        let total_fail: u32 = arms.values().map(|a| a.failure_count).sum();
        let total = total_success + total_fail;
        if total == 0 {
            return;
        }
        let success_rate = total_success as f64 / total as f64;
        let mut c = self.c.write();
        if success_rate > 0.9 {
            *c = (*c * 0.95).max(0.5);
        } else if success_rate < 0.7 {
            *c = (*c * 1.05).min(3.0);
        }
        log::debug!("Bandit 自调优: success_rate={:.2}, c={:.3}", success_rate, *c);
    }

    /// 对一组候选按 UCB 评分排序，返回排序后的索引
    pub fn rank_candidates(
        &self,
        candidates: &[(String, String, Option<String>)],
    ) -> Vec<usize> {
        let arms = self.arms.read();
        let rounds = *self.rounds.read();
        let c = *self.c.read();
        let mut scored: Vec<(usize, f64)> = candidates
            .iter()
            .enumerate()
            .map(|(i, (provider, model, conn))| {
                let key = Self::make_key(provider, model, conn.as_deref());
                let score = arms.get(&key).map(|a| a.ucb_score(rounds, c)).unwrap_or(1.0);
                (i, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// 记数成功
    pub fn record_success(&self, provider: &str, model: &str, connection_id: Option<&str>, latency_ms: f64) {
        let key = Self::make_key(provider, model, connection_id);
        let mut arms = self.arms.write();
        let arm = arms.entry(key.clone()).or_insert_with(ScoreArm::new);
        arm.record_success(latency_ms);
        *self.rounds.write() += 1;
        *self.last_success.write() = Some(key);
        drop(arms);
        self.auto_tune();
    }

    /// 获取 auto 路由最近成功的候选
    pub fn get_last_success(&self) -> Option<(String, String, Option<String>)> {
        let key = self.last_success.read().clone()?;
        let parts: Vec<&str> = key.splitn(3, ':').collect();
        if parts.len() < 3 {
            return None;
        }
        let connection_id = if parts[2].is_empty() {
            None
        } else {
            Some(parts[2].to_string())
        };
        Some((parts[0].to_string(), parts[1].to_string(), connection_id))
    }

    /// 清除最近成功记忆（当该模型失败时调用）
    pub fn clear_last_success(&self) {
        *self.last_success.write() = None;
    }

    /// 计数失败
    pub fn record_failure(&self, provider: &str, model: &str, connection_id: Option<&str>) {
        let key = Self::make_key(provider, model, connection_id);
        let mut arms = self.arms.write();
        let arm = arms.entry(key).or_insert_with(ScoreArm::new);
        arm.record_failure();
        *self.rounds.write() += 1;
        drop(arms);
        self.auto_tune();
    }

    /// 获取某候选的统计信息
    pub fn get_stats(&self, provider: &str, model: &str, connection_id: Option<&str>) -> Option<(u32, u32, f64)> {
        let key = Self::make_key(provider, model, connection_id);
        let arms = self.arms.read();
        arms.get(&key).map(|a| (a.success_count, a.failure_count, a.last_latency_ms))
    }

    /// 获取当前探索常数 C（用于调试/展示）
    pub fn exploration_constant(&self) -> f64 {
        *self.c.read()
    }
}
