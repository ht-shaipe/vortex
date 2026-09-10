pub mod combo;
pub mod strategies;
pub mod resilience;

use crate::db::models::ResolvedComboTarget;
use crate::routing::strategies::RoutingStrategy;
use std::collections::HashMap;

pub struct RoutingEngine {
    strategies: HashMap<String, Box<dyn RoutingStrategy + Send + Sync>>,
}

impl RoutingEngine {
    pub fn new() -> Self {
        let mut strategies: HashMap<String, Box<dyn RoutingStrategy + Send + Sync>> = HashMap::new();
        strategies.insert("priority".into(), Box::new(strategies::Priority));
        strategies.insert("fill_first".into(), Box::new(strategies::FillFirst::new()));
        strategies.insert("weighted".into(), Box::new(strategies::Weighted));
        strategies.insert("round_robin".into(), Box::new(strategies::RoundRobin::new()));
        strategies.insert("p2c".into(), Box::new(strategies::P2C));
        strategies.insert("least_used".into(), Box::new(strategies::LeastUsed::new()));
        strategies.insert("random".into(), Box::new(strategies::Random));
        strategies.insert("strict_random".into(), Box::new(strategies::StrictRandom));
        strategies.insert("cost_optimized".into(), Box::new(strategies::CostOptimized));
        strategies.insert("headroom".into(), Box::new(strategies::Headroom));
        strategies.insert("reset_window".into(), Box::new(strategies::ResetWindow));
        strategies.insert("reset_aware".into(), Box::new(strategies::ResetAware));
        strategies.insert("context_relay".into(), Box::new(strategies::ContextRelay));
        strategies.insert("context_optimized".into(), Box::new(strategies::ContextOptimized));
        strategies.insert("lkgp".into(), Box::new(strategies::LKGP::new()));
        strategies.insert("auto".into(), Box::new(strategies::Auto));
        strategies.insert("fusion".into(), Box::new(strategies::Fusion));
        Self { strategies }
    }

    pub fn resolve(
        &self,
        strategy: &str,
        targets: &[ResolvedComboTarget],
        context: &crate::db::models::RoutingContext,
    ) -> Vec<ResolvedComboTarget> {
        if let Some(s) = self.strategies.get(strategy) {
            match s.select(targets, context) {
                Ok(ordered) => ordered.into_iter().cloned().collect(),
                Err(_) => targets.to_vec(),
            }
        } else {
            targets.to_vec()
        }
    }

    pub fn list_strategies(&self) -> Vec<StrategyInfo> {
        vec![
            StrategyInfo { id: "priority".into(), name: "Priority".into(), description: "Try targets in order, use first available".into() },
            StrategyInfo { id: "fill_first".into(), name: "Fill First".into(), description: "Fill each target's quota before moving on".into() },
            StrategyInfo { id: "weighted".into(), name: "Weighted".into(), description: "Weighted random by per-target weight".into() },
            StrategyInfo { id: "round_robin".into(), name: "Round Robin".into(), description: "Cycle through targets in order".into() },
            StrategyInfo { id: "p2c".into(), name: "Power of Two Choices".into(), description: "Random load balancing with P2C".into() },
            StrategyInfo { id: "least_used".into(), name: "Least Used".into(), description: "Pick target with lowest current load".into() },
            StrategyInfo { id: "random".into(), name: "Random".into(), description: "Uniform random pick (deduplicated)".into() },
            StrategyInfo { id: "strict_random".into(), name: "Strict Random".into(), description: "Random without deduplicating repeats".into() },
            StrategyInfo { id: "cost_optimized".into(), name: "Cost Optimized".into(), description: "Minimize $ per request from live pricing".into() },
            StrategyInfo { id: "headroom".into(), name: "Headroom".into(), description: "Pick target with most remaining quota".into() },
            StrategyInfo { id: "reset_window".into(), name: "Reset Window".into(), description: "Prefer target whose quota resets soonest".into() },
            StrategyInfo { id: "reset_aware".into(), name: "Reset Aware".into(), description: "Rank by quota reset time, short windows first".into() },
            StrategyInfo { id: "context_relay".into(), name: "Context Relay".into(), description: "Hand off context across targets for long conversations".into() },
            StrategyInfo { id: "context_optimized".into(), name: "Context Optimized".into(), description: "Best fit for current context size".into() },
            StrategyInfo { id: "lkgp".into(), name: "LKGP".into(), description: "Last-Known-Good-Path — sticky to last successful target".into() },
            StrategyInfo { id: "auto".into(), name: "Auto".into(), description: "9-factor live scoring across every connection".into() },
            StrategyInfo { id: "fusion".into(), name: "Fusion".into(), description: "Fan out to panel of models + judge synthesizes answer".into() },
        ]
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}
