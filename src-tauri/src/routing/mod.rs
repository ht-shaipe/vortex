//! 路由策略引擎模块入口
//!
//! 包含熔断器与弹性管理子模块，负责连接健康状态跟踪与故障隔离。

/// 熔断器与弹性管理器，实现三态熔断保护
pub mod resilience;
/// 内存速率限制器，滑动窗口 RPM/RPD/TPM/TPD 计数
pub mod rate_limiter;
/// Thompson 采样 bandit 路由评分
pub mod bandit;
