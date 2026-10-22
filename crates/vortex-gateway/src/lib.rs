//! Vortex HTTP 网关服务层。
//!
//! 包含应用全局状态 [`AppState`]、HTTP API 端点（api）、智能体集成（agent_integrations），
//! 以及服务器启动函数 [`start_api_server`]。
//! AppState 实现 [`vortex_router::context::EngineContext`] trait，供代理引擎访问。

pub mod api;
pub mod agent_integrations;
pub mod logging;
pub mod services;
pub mod tls;

use actix_cors::Cors;
use actix_web::{dev::ServerHandle, web, App, HttpServer, middleware as actix_mw};
use std::sync::{mpsc, Arc};
use vortex_store::config::AppConfig;
use vortex_store::db;
use vortex_router::context::EngineContext;
use vortex_router::providers::ProviderRegistry;
use vortex_router::proxy;
use vortex_router::proxy::web_bridge::WebChatBridge;
use vortex_router::routing;

/// 默认日志过滤器：vortex 自身 info；obscura 引擎（脚本执行/样式表等
/// 过程日志，页面加载时高频）与 actix 服务器（worker 启停）静音到 warn；
/// obscura::console（页面 JS console 捕获，如目标站的 rsbuild 资源重试、
/// "Dynamic script fetch error: HTTP 0" 等，属引擎网络特性导致的正常噪音）
/// 整体关闭。可用 RUST_LOG 环境变量覆盖（如 RUST_LOG=debug 观察全部细节）。
pub const DEFAULT_LOG_FILTER: &str =
    "info,obscura=warn,obscura::console=off,actix_server=warn,actix_web=warn";

/// 应用全局状态。
///
/// 该结构体在应用启动时创建一次，通过 `Arc` 在多个线程与 Tauri 命令之间共享。
/// 包含数据库连接池、配置、提供商注册表、代理引擎、弹性路由管理器等核心组件。
pub struct AppState {
    /// SQLite 数据库连接池。
    pub db_pool: db::core::DbPool,
    /// 应用运行时配置。
    pub config: AppConfig,
    /// AI 提供商注册表，管理已配置的提供商实例。
    pub provider_registry: ProviderRegistry,
    /// 代理引擎，处理请求的代理转发逻辑。
    pub proxy_engine: proxy::engine::ProxyEngine,
    /// 路由弹性管理器，负责故障转移、负载均衡等路由策略。
    pub resilience_manager: routing::resilience::ResilienceManager,
    /// 内存速率限制器，滑动窗口 RPM/RPD/TPM/TPD 计数
    pub rate_limiter: routing::rate_limiter::RateLimiter,
    /// Thompson 采样 bandit 路由评分器
    pub bandit_router: routing::bandit::BanditRouter,
    /// 模型占用追踪器，auto 路由跨智能体软避让
    pub occupancy: routing::occupancy::OccupancyTracker,
    /// 粘性会话管理器
    pub sticky_session: proxy::sticky_session::StickySessionManager,
    /// 网页版平台桥接（obscura worker 集合 + CDP 授权）
    pub web_bridge: Arc<services::web_provider_service::WebProviderBridge>,
    /// 加密密钥的字节序列，用于加密/解密存储中的敏感数据。
    pub encryption_key: Vec<u8>,
    /// 代理服务器的句柄，存储在互斥锁中以便启停控制。
    pub proxy_handle: parking_lot::Mutex<Option<ServerHandle>>,
    /// 代理服务器监听端口。
    pub proxy_port: u16,
}

impl EngineContext for AppState {
    fn db_pool(&self) -> &db::core::DbPool {
        &self.db_pool
    }
    fn config(&self) -> &AppConfig {
        &self.config
    }
    fn provider_registry(&self) -> &ProviderRegistry {
        &self.provider_registry
    }
    fn sticky_session(&self) -> &proxy::sticky_session::StickySessionManager {
        &self.sticky_session
    }
    fn encryption_key(&self) -> &[u8] {
        &self.encryption_key
    }
    fn resilience_manager(&self) -> &routing::resilience::ResilienceManager {
        &self.resilience_manager
    }
    fn rate_limiter(&self) -> &routing::rate_limiter::RateLimiter {
        &self.rate_limiter
    }
    fn bandit_router(&self) -> &routing::bandit::BanditRouter {
        &self.bandit_router
    }
    fn occupancy(&self) -> &routing::occupancy::OccupancyTracker {
        &self.occupancy
    }
    fn web_bridge(&self) -> Option<Arc<dyn WebChatBridge>> {
        Some(self.web_bridge.clone())
    }
}

/// 创建 HTTP 客户端（awc + rustls）。
///
/// 必须在 actix runtime 上下文中调用。
pub fn create_awc_client() -> awc::Client {
    let awc_connector = awc::Connector::new()
        .timeout(std::time::Duration::from_secs(10));

    awc::Client::builder()
        .connector(awc_connector)
        .timeout(std::time::Duration::from_secs(300))
        .add_default_header(("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"))
        .add_default_header(("Accept", "application/json"))
        .finish()
}

/// 创建应用全局状态。
pub fn create_app_state(cfg: AppConfig) -> vortex_store::error::Result<Arc<AppState>> {
    let db_pool = db::core::init_pool(&cfg.data_dir, cfg.encryption_key.clone())?;
    db::core::run_migrations(&db_pool)?;
    {
        let conn = db::core::get_conn(&db_pool)?;
        db::settings::ensure_security_defaults(&conn)?;
    }

    // TLS 开关解析：环境变量显式设置时优先，否则读数据库持久化设置（设置页开关）
    let mut cfg = cfg;
    if !cfg.tls_env_explicit {
        if let Ok(conn) = db::core::get_conn(&db_pool) {
            if let Ok(Some(general)) = db::settings::get(&conn, "settings", "general") {
                if let Some(v) = general.get("tls_enabled").and_then(|v| v.as_bool()) {
                    cfg.tls_enabled = v;
                }
            }
        }
    }

    let provider_registry = ProviderRegistry::new();
    let proxy_engine = proxy::engine::ProxyEngine::new();
    let resilience_manager = routing::resilience::ResilienceManager::new();
    let rate_limiter = routing::rate_limiter::RateLimiter::new();
    let bandit_router = routing::bandit::BanditRouter::new();
    let sticky_session = proxy::sticky_session::StickySessionManager::new();
    let encryption_key = cfg.encryption_key_bytes();
    let proxy_port = cfg.port;

    // 网页版平台 worker：环境变量 VORTEX_WEB_PLATFORMS（逗号分隔）指定
    // 启动即拉起的平台（默认 doubao）；未列出的已知平台在「网页版账号」
    // 页发起授权时按需启动（WebProviderBridge::ensure_platform）
    let web_platforms = std::env::var("VORTEX_WEB_PLATFORMS")
        .unwrap_or_else(|_| "doubao".to_string());
    let web_bridge = Arc::new(services::web_provider_service::WebProviderBridge::new(
        &cfg.data_dir.to_string_lossy(),
        &web_platforms,
    ));

    Ok(Arc::new(AppState {
        db_pool,
        config: cfg,
        provider_registry,
        proxy_engine,
        resilience_manager,
        rate_limiter,
        bandit_router,
        occupancy: routing::occupancy::OccupancyTracker::new(),
        sticky_session,
        web_bridge,
        encryption_key,
        proxy_handle: parking_lot::Mutex::new(None),
        proxy_port,
    }))
}

/// 启动内嵌的 HTTP API 服务器。
///
/// 在独立线程中创建 Actix 运行时并启动 HTTP 服务器，注册 `/v1/*` 代理 API 和 `/api/*` 管理 API。
pub fn start_api_server(
    state: Arc<AppState>,
    port: u16,
) -> ServerHandle {
    let (handle_tx, handle_rx) = mpsc::channel();

    std::thread::spawn(move || {
        let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
        rt.block_on(async move {
            let state_clone = state.clone();
            let server = HttpServer::new(move || {
                let cors = Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600);

                App::new()
                    .wrap(cors)
                    .wrap(actix_mw::Logger::default())
                    .app_data(web::Data::new(state_clone.clone()))
                    .service(
                        web::scope("/v1")
                            .route("/chat/completions", web::post().to(api::v1::chat::chat_completions))
                            .route("/messages", web::post().to(api::v1::messages::anthropic_messages))
                            .route("/responses", web::post().to(api::v1::responses::responses))
                            .route("/models", web::get().to(api::v1::models::list_models))
                            .route("/embeddings", web::post().to(api::v1::embeddings::create_embeddings))
                            .route("/images/generations", web::post().to(api::v1::images::create_images))
                            .route("/mcp/tools", web::get().to(api::v1::mcp::mcp_tools))
                            .route("/mcp/call", web::post().to(api::v1::mcp::mcp_call))
                    )
                    .service(
                        web::scope("/api")
                            .route("/providers", web::get().to(api::management::providers::list_providers))
                            .route("/providers", web::post().to(api::management::providers::create_provider))
                            .route("/providers/{id}", web::get().to(api::management::providers::get_provider))
                            .route("/providers/{id}", web::patch().to(api::management::providers::update_provider))
                            .route("/providers/{id}", web::delete().to(api::management::providers::delete_provider))
                            .route("/providers/{id}/test", web::post().to(api::management::providers::test_provider))
                            .route("/providers/{id}/apikey", web::get().to(api::management::providers::get_api_key))
                            .route("/providers/preview-models", web::post().to(api::management::providers::preview_models))
                            .route("/keys", web::get().to(api::management::keys::list_keys))
                            .route("/keys", web::post().to(api::management::keys::create_key))
                            .route("/keys/{id}", web::get().to(api::management::keys::get_key))
                            .route("/keys/{id}", web::delete().to(api::management::keys::delete_key))
                            .route("/usage", web::get().to(api::management::usage::get_usage))
                            .route("/usage/stats", web::get().to(api::management::usage::get_stats))
                            .route("/settings", web::get().to(api::management::settings::get_settings))
                            .route("/settings", web::patch().to(api::management::settings::update_settings))
                            .route("/free-tokens", web::get().to(api::management::free_tokens::list_sites))
                            .route("/free-tokens", web::post().to(api::management::free_tokens::create_site))
                            .route("/free-tokens/{id}", web::delete().to(api::management::free_tokens::delete_site))
                            .route("/web-providers", web::get().to(api::management::web_providers::list_web_providers))
                            .route("/web-providers/{platform}/login/cdp", web::post().to(api::management::web_providers::start_cdp_login))
                            .route("/web-providers/{platform}/login/cdp", web::get().to(api::management::web_providers::cdp_login_status))
                            .route("/web-providers/{platform}/login/cdp/finish", web::post().to(api::management::web_providers::finish_cdp_login))
                            .route("/web-providers/{platform}/login/cdp/cancel", web::post().to(api::management::web_providers::cancel_cdp_login))
                            .route("/web-providers/{platform}/debug", web::get().to(api::management::web_providers::platform_debug))
                            .route("/web-providers/{platform}/reset", web::post().to(api::management::web_providers::platform_reset))
                            .route("/model-aliases", web::get().to(api::management::model_aliases::list_aliases))
                            .route("/model-aliases", web::post().to(api::management::model_aliases::create_alias))
                            .route("/model-aliases/auto-generate", web::post().to(api::management::model_aliases::auto_generate))
                            .route("/model-aliases/reorder", web::post().to(api::management::model_aliases::reorder_aliases))
                            .route("/model-aliases/{id}", web::patch().to(api::management::model_aliases::update_alias))
                            .route("/model-aliases/{id}", web::delete().to(api::management::model_aliases::delete_alias))
                            .route("/rate-limits", web::get().to(api::management::rate_limits::list_rate_limits))
                            .route("/rate-limits", web::post().to(api::management::rate_limits::upsert_rate_limit))
                            .route("/rate-limits/{id}", web::delete().to(api::management::rate_limits::delete_rate_limit))
                            .route("/rate-limits/usage", web::get().to(api::management::rate_limits::list_usage))
                            .route("/rate-limits/cleanup", web::post().to(api::management::rate_limits::cleanup_usage))
                            .route("/bulk-keys/import", web::post().to(api::management::bulk_keys::bulk_import))
                            .route("/bulk-keys/export", web::post().to(api::management::bulk_keys::bulk_export))
                            .route("/routing-profiles", web::get().to(api::management::routing_profiles::list_profiles))
                            .route("/routing-profiles", web::post().to(api::management::routing_profiles::create_profile))
                            .route("/routing-profiles/{id}", web::delete().to(api::management::routing_profiles::delete_profile))
                            .route("/tos-reviews", web::get().to(api::management::tos_review::list_tos_reviews))
                            .route("/backups", web::get().to(api::management::backup::list_backups))
                            .route("/backups", web::post().to(api::management::backup::create_backup))
                            .route("/model-catalog", web::get().to(api::management::model_catalog::list_catalog))
                            .route("/model-catalog/refresh-from-connections", web::post().to(api::management::model_catalog::refresh_from_connections_handler))
                            .route("/model-catalog/cleanup", web::post().to(api::management::model_catalog::cleanup_catalog))
                            .route("/maintenance/storage", web::get().to(api::management::maintenance::storage_info))
                            .route("/maintenance/storage/cleanup", web::post().to(api::management::maintenance::storage_cleanup))
                            .route("/maintenance/log/clear", web::post().to(api::management::maintenance::log_clear))
                            .route("/latency-stats", web::get().to(api::management::latency_stats::latency_stats))
                            .route("/playground", web::post().to(api::management::playground::playground))
                            .route("/cost-analysis", web::get().to(api::management::cost_analysis::cost_analysis))
                            .route("/recommendations", web::get().to(api::management::recommendations::recommendations))
                            .route("/compressed-content", web::get().to(api::management::compressed_content::list_compressed_content))
                            .route("/compressed-content/{hash}", web::get().to(api::management::compressed_content::get_compressed_content))
                            .route("/key-permissions", web::get().to(api::management::key_permissions::list_permissions))
                            .route("/key-permissions", web::post().to(api::management::key_permissions::create_permission))
                            .route("/key-permissions/{id}", web::delete().to(api::management::key_permissions::delete_permission))
                            .route("/health", web::get().to(api::management::health::health_check))
                            .route("/", web::get().to(api::management::health::index))
                            .route("/system/status", web::get().to(api::management::system::get_system_status))
                            .route("/system/proxy/start", web::post().to(api::management::system::start_proxy))
                            .route("/system/proxy/stop", web::post().to(api::management::system::stop_proxy))
                            .route("/agents/detect", web::get().to(api::management::agents::detect_agents))
                            .route("/agents/preview", web::post().to(api::management::agents::preview_config))
                            .route("/agents/apply", web::post().to(api::management::agents::apply_config))
                            .route("/agents/restore", web::post().to(api::management::agents::restore_config))
                            .route("/agents/backups", web::get().to(api::management::agents::list_backups))
                            .route("/chat/cancel", web::post().to(api::management::chat::cancel_chat_stream))
                    )
                    // 未匹配的路径（浏览器直接访问根/任意路径）返回版本标识
                    .default_service(web::to(api::management::health::index_fallback))
            });

            // 双端口模式：主端口始终提供 HTTP（兼容全部客户端）；
            // TLS 开启时另在 tls_https_port（默认 port+1）提供 HTTPS。
            // HTTPS 端口由本进程先绑定 std listener 再交给 actix（消除预检竞态），
            // 端口被占用（如上一实例未退净）时降级为仅 HTTP 并告警，不崩溃。
            let server = server
                .bind(format!("0.0.0.0:{}", port))
                .expect("Failed to bind server");
            let srv = if state.config.tls_enabled {
                let tls_cfg = tls::ensure_tls(&state.config)
                    .unwrap_or_else(|e| panic!("TLS 初始化失败: {}（可检查 VORTEX_TLS_CERT/VORTEX_TLS_KEY 或删除数据目录下 tls/ 重试）", e));
                let https_port = state.config.tls_https_port;
                match std::net::TcpListener::bind(("0.0.0.0", https_port)) {
                    Ok(lst) => {
                        log::info!("[TLS] HTTPS 已启用，监听 0.0.0.0:{}（HTTP 继续保留在 0.0.0.0:{}）", https_port, port);
                        server
                            .listen_rustls(lst, tls_cfg)
                            .expect("Failed to attach TLS listener")
                            .run()
                    }
                    Err(e) => {
                        log::warn!("[TLS] HTTPS 端口 {} 绑定失败（{}），本次仅提供 HTTP 服务", https_port, e);
                        server.run()
                    }
                }
            } else {
                server.run()
            };
            let handle = srv.handle();
            let _ = handle_tx.send(handle);

            // 启动后自动从已配置连接拉取最新模型列表（延迟 10 秒），每 12 小时定期拉取
            {
                let pool = state.db_pool.clone();
                let enc_key = state.encryption_key.clone();
                let provider_defaults: std::collections::HashMap<String, (String, String)> = state
                    .provider_registry
                    .list()
                    .into_iter()
                    .map(|d| (d.id.clone(), (d.base_url.clone(), d.models_path.clone())))
                    .collect();
                actix_rt::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    loop {
                        match vortex_router::providers::catalog_sync::refresh_from_connections(&pool, &enc_key, &provider_defaults).await {
                            Ok((s, t, _)) => {
                                if s > 0 {
                                    log::info!("从连接拉取模型目录完成: {} 个连接, {} 条模型", s, t);
                                }
                            }
                            Err(e) => log::warn!("从连接拉取模型目录失败: {}", e),
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(43200)).await;
                    }
                });
            }

            // 存储预算巡检：统计数据（usage_history）全保留，压缩存档/
            // 日志大表限额自动清理 + VACUUM 收缩（含过期存档清理，取代
            // 原先"先睡 24h 才首次执行"的过期清理任务——桌面应用很少
            // 连跑 24h，旧任务实际从未执行过）。启动 2 分钟后首跑，
            // 之后每 6 小时巡检；预算默认 200MB（VORTEX_STORAGE_BUDGET_MB）
            {
                let pool = state.db_pool.clone();
                actix_rt::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                    loop {
                        let budget_mb = vortex_store::db::maintenance::budget_mb_from_env();
                        let pool_clone = pool.clone();
                        let report = tokio::task::spawn_blocking(move || {
                            let conn = vortex_store::db::core::get_conn(&pool_clone)
                                .map_err(|e| e.to_string())?;
                            vortex_store::db::maintenance::enforce_storage_budget(
                                &conn,
                                budget_mb * 1024 * 1024,
                                false,
                            )
                            .map_err(|e| e.to_string())
                        })
                        .await;
                        match report {
                            Ok(Ok(r)) if r.expired_deleted + r.compressed_deleted + r.call_log_trimmed + r.call_log_deleted > 0 => {
                                log::info!(
                                    "存储预算清理完成（预算 {budget_mb}MB）：过期存档 {} 条、超预算存档 {} 条、\
                                     日志清字段 {} 行/删 {} 行；库 {}MB → {}MB",
                                    r.expired_deleted, r.compressed_deleted,
                                    r.call_log_trimmed, r.call_log_deleted,
                                    r.before_bytes / 1024 / 1024, r.after_bytes / 1024 / 1024
                                );
                            }
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => log::warn!("存储预算巡检失败: {e}"),
                            Err(e) => log::warn!("存储预算巡检任务异常: {e}"),
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(21600)).await;
                    }
                });
            }

            srv.await.expect("Server error");
        });
    });

    handle_rx.recv().expect("Failed to receive server handle")
}
