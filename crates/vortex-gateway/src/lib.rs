//! Vortex HTTP 网关服务层。
//!
//! 包含应用全局状态 [`AppState`]、HTTP API 端点（api）、智能体集成（agent_integrations），
//! 以及服务器启动函数 [`start_api_server`]。
//! AppState 实现 [`vortex_router::context::EngineContext`] trait，供代理引擎访问。

pub mod api;
pub mod agent_integrations;

use actix_cors::Cors;
use actix_web::{dev::ServerHandle, web, App, HttpServer, middleware as actix_mw};
use std::sync::{mpsc, Arc};
use vortex_store::config::AppConfig;
use vortex_store::db;
use vortex_router::context::EngineContext;
use vortex_router::providers::ProviderRegistry;
use vortex_router::proxy;
use vortex_router::routing;

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

    let provider_registry = ProviderRegistry::new();
    let proxy_engine = proxy::engine::ProxyEngine::new();
    let resilience_manager = routing::resilience::ResilienceManager::new();
    let rate_limiter = routing::rate_limiter::RateLimiter::new();
    let bandit_router = routing::bandit::BanditRouter::new();
    let sticky_session = proxy::sticky_session::StickySessionManager::new();
    let encryption_key = cfg.encryption_key_bytes();
    let proxy_port = cfg.port;

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
                    )
            })
            .bind(format!("0.0.0.0:{}", port))
            .expect("Failed to bind server");

            let srv = server.run();
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

            // 定期清理过期压缩内容（每 24 小时）
            {
                let pool = state.db_pool.clone();
                actix_rt::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
                        let pool_clone = pool.clone();
                        let _ = tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
                            let conn = vortex_store::db::core::get_conn(&pool_clone).map_err(|e| e.to_string())?;
                            let n = vortex_store::db::compressed_content::cleanup_expired(&conn).map_err(|e| e.to_string())?;
                            if n > 0 { log::info!("清理过期压缩内容: {} 条", n); }
                            Ok(())
                        }).await;
                    }
                });
            }

            srv.await.expect("Server error");
        });
    });

    handle_rx.recv().expect("Failed to receive server handle")
}
