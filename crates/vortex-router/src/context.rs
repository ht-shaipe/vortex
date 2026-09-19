//! 引擎上下文 trait。
//!
//! [`EngineContext`] 定义代理引擎访问应用状态所需的接口，
//! 上层 crate（如 vortex-gateway）的 AppState 实现此 trait，
//! 使引擎不依赖具体状态类型，实现核心逻辑与应用层解耦。

use vortex_store::config::AppConfig;
use vortex_store::db::core::DbPool;
use crate::providers::ProviderRegistry;
use crate::proxy::sticky_session::StickySessionManager;
use crate::routing::resilience::ResilienceManager;
use crate::routing::rate_limiter::RateLimiter;
use crate::routing::bandit::BanditRouter;
use crate::routing::occupancy::OccupancyTracker;

/// 代理引擎上下文。
///
/// 提供引擎处理请求时所需的全部组件引用：数据库连接池、配置、提供商注册表、
/// 路由策略管理器等。由上层 AppState 实现，引擎通过 trait 接口访问，
/// 不依赖具体类型。
pub trait EngineContext {
    /// SQLite 数据库连接池
    fn db_pool(&self) -> &DbPool;
    /// 应用运行时配置
    fn config(&self) -> &AppConfig;
    /// AI 提供商注册表
    fn provider_registry(&self) -> &ProviderRegistry;
    /// 粘性会话管理器
    fn sticky_session(&self) -> &StickySessionManager;
    /// 加密密钥字节序列
    fn encryption_key(&self) -> &[u8];
    /// 路由弹性管理器（熔断器）
    fn resilience_manager(&self) -> &ResilienceManager;
    /// 内存速率限制器
    fn rate_limiter(&self) -> &RateLimiter;
    /// Thompson 采样 bandit 路由评分器
    fn bandit_router(&self) -> &BanditRouter;
    /// 模型占用追踪器
    fn occupancy(&self) -> &OccupancyTracker;
}
