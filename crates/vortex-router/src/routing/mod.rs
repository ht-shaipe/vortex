//! 路由策略引擎模块入口
//!
//! 包含熔断器与弹性管理子模块，负责连接健康状态跟踪与故障隔离。

/// 熔断器与弹性管理器，实现三态熔断保护
pub mod resilience;
/// 内存速率限制器，滑动窗口 RPM/RPD/TPM/TPD 计数
pub mod rate_limiter;
/// Thompson 采样 bandit 路由评分
pub mod bandit;
/// 模型占用追踪器，auto 路由跨智能体软避让
pub mod occupancy;
/// 终端粘性模型追踪器，auto 路由按终端（agent 指纹）粘合模型
pub mod agent_sticky;
