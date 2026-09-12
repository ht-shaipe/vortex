//! Vortex AI Gateway 核心库模块。
//!
//! 本文件是整个应用的核心入口，负责：
//! - 声明各子模块（数据库、配置、错误处理、提供商、路由、代理、翻译、API、Tauri 命令等）
//! - 定义应用全局状态 [`AppState`]
//! - 提供状态初始化函数 [`create_app_state`]
//! - 提供内嵌 HTTP API 服务器启动函数 [`start_api_server`]
//! - 提供主入口函数 [`run`]：加载配置 → 创建状态 → 启动 API → 启动 Tauri 桌面应用（含系统托盘）

mod db;
mod error;
mod config;
mod providers;
mod routing;
mod proxy;
mod remote_proxy;
mod translator;
mod api;
mod tauri_cmds;

use actix_cors::Cors;
use actix_web::{dev::ServerHandle, web, App, HttpServer, middleware as actix_mw};
use std::sync::{mpsc, Arc};

/// 应用全局状态。
///
/// 该结构体在应用启动时创建一次，通过 `Arc` 在多个线程与 Tauri 命令之间共享。
/// 包含数据库连接池、配置、提供商注册表、代理引擎、弹性路由管理器、HTTP 客户端等核心组件。
pub struct AppState {
    /// SQLite 数据库连接池。
    pub db_pool: db::core::DbPool,
    /// 应用运行时配置。
    pub config: config::AppConfig,
    /// AI 提供商注册表，管理已配置的提供商实例。
    pub provider_registry: providers::ProviderRegistry,
    /// 代理引擎，处理请求的代理转发逻辑（读写锁保护，允许多读单写）。
    pub proxy_engine: parking_lot::RwLock<proxy::engine::ProxyEngine>,
    /// 路由弹性管理器，负责故障转移、负载均衡等路由策略。
    pub resilience_manager: routing::resilience::ResilienceManager,
    /// 共享的 HTTP 客户端，用于向上游 AI 提供商发起请求。
    pub http_client: reqwest::Client,
    /// 加密密钥的字节序列，用于加密/解密存储中的敏感数据。
    pub encryption_key: Vec<u8>,
    /// 代理服务器的句柄，存储在互斥锁中以便启停控制。
    pub proxy_handle: parking_lot::Mutex<Option<ServerHandle>>,
    /// 代理服务器监听端口。
    pub proxy_port: u16,
}

/// 创建应用全局状态。
///
/// 执行以下初始化步骤：
/// 1. 初始化 SQLite 数据库连接池
/// 2. 运行数据库迁移
/// 3. 创建提供商注册表、代理引擎、弹性路由管理器
/// 4. 构建共享 HTTP 客户端（配置连接超时与连接池参数）
/// 5. 派生加密密钥字节序列
///
/// # 参数
///
/// - `cfg`：已加载的应用配置。
///
/// # 返回值
///
/// 返回包装在 `Arc` 中的 [`AppState`]，以便多线程共享；若初始化失败则返回 [`error::AppError`]。
pub fn create_app_state(cfg: config::AppConfig) -> error::Result<Arc<AppState>> {
    // 初始化数据库连接池
    let db_pool = db::core::init_pool(&cfg.data_dir, cfg.encryption_key.clone())?;
    // 运行数据库迁移
    db::core::run_migrations(&db_pool)?;

    // 创建提供商注册表
    let provider_registry = providers::ProviderRegistry::new();
    // 创建代理引擎
    let proxy_engine = proxy::engine::ProxyEngine::new();
    // 创建弹性路由管理器
    let resilience_manager = routing::resilience::ResilienceManager::new();
    // 不设全局总超时：reqwest 的总超时会掐断长时间运行的流式响应。
    // 超时保护改为多级：
    //   - 连接超时 10s（此处）
    //   - 流式首字节 60s / 非流式整体 300s（executor.rs 按请求应用）
    //   - 流式逐块 120s（sse.rs 按次计时）
    let http_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| error::AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    // 派生加密密钥字节序列
    let encryption_key = cfg.encryption_key_bytes();

    // 记录代理端口
    let proxy_port = cfg.port;

    // 组装并返回应用状态
    Ok(Arc::new(AppState {
        db_pool,
        config: cfg,
        provider_registry,
        proxy_engine: parking_lot::RwLock::new(proxy_engine),
        resilience_manager,
        http_client,
        encryption_key,
        proxy_handle: parking_lot::Mutex::new(None),
        proxy_port,
    }))
}

/// 启动内嵌的 HTTP API 服务器。
///
/// 在独立线程中创建 Actix 运行时并启动 HTTP 服务器，注册两组路由：
/// - `/v1/*`：OpenAI / Anthropic 兼容的代理 API（聊天、消息、模型、嵌入、图像）
/// - `/api/*`：管理 API（提供商、密钥、用量、设置、免费 Token、健康检查）
///
/// 服务器绑定到 `0.0.0.0:{port}`，启用 CORS（允许任意来源/方法/头）与请求日志中间件。
///
/// # 参数
///
/// - `state`：应用全局状态的共享引用。
/// - `port`：服务器监听端口。
///
/// # 返回值
///
/// 返回 [`ServerHandle`]，可用于后续停止服务器。
pub fn start_api_server(
    state: Arc<AppState>,
    port: u16,
) -> ServerHandle {
    // 创建通道用于在线程间传递服务器句柄
    let (handle_tx, handle_rx) = mpsc::channel();

    // 在独立线程中启动 Actix 运行时与 HTTP 服务器
    std::thread::spawn(move || {
        let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
        rt.block_on(async move {
            let state_clone = state.clone();
            let server = HttpServer::new(move || {
                // 配置 CORS：允许任意来源、方法、头，预检缓存 1 小时
                let cors = Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600);

                App::new()
                    .wrap(cors)
                    .wrap(actix_mw::Logger::default())
                    // 注入应用全局状态
                    .app_data(web::Data::new(state_clone.clone()))
                    // /v1/* 代理 API 路由组
                    .service(
                        web::scope("/v1")
                            .route("/chat/completions", web::post().to(api::v1::chat::chat_completions))
                            .route("/messages", web::post().to(api::v1::messages::anthropic_messages))
                            .route("/models", web::get().to(api::v1::models::list_models))
                            .route("/embeddings", web::post().to(api::v1::embeddings::create_embeddings))
                            .route("/images/generations", web::post().to(api::v1::images::create_images))
                    )
                    // /api/* 管理 API 路由组
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
                            .route("/model-aliases/{id}", web::patch().to(api::management::model_aliases::update_alias))
                            .route("/model-aliases/{id}", web::delete().to(api::management::model_aliases::delete_alias))
                            .route("/health", web::get().to(api::management::health::health_check))
                    )
            })
            .bind(format!("0.0.0.0:{}", port))
            .expect("Failed to bind server");

            // 启动服务器并获取句柄
            let srv = server.run();
            let handle = srv.handle();
            // 通过通道将句柄发送给主线程
            let _ = handle_tx.send(handle);
            // 阻塞等待服务器运行结束
            srv.await.expect("Server error");
        });
    });

    // 接收并返回服务器句柄
    handle_rx.recv().expect("Failed to receive server handle")
}


/// 应用主入口函数。
///
/// 执行完整的启动流程：
/// 1. 从环境变量加载应用配置
/// 2. 创建应用全局状态（初始化数据库、HTTP 客户端等）
/// 3. 启动内嵌 HTTP API 服务器
/// 4. 启动 Tauri 桌面应用，注册插件、命令处理器与系统托盘
///
/// 该函数同时作为桌面端入口和移动端入口（通过 `#[cfg_attr(mobile, tauri::mobile_entry_point)]`）。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志：默认 info 级别，可通过 RUST_LOG 环境变量覆盖。
    // 未初始化时所有 log:: 调用都会被丢弃，路由回退、上游错误等关键信息不可见。
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();

    // 加载应用配置
    let cfg = config::AppConfig::load();
    // 创建应用全局状态
    let state = create_app_state(cfg.clone()).expect("Failed to initialize app state");

    // 启动 HTTP API 服务器并保存句柄到全局状态
    let port = cfg.port;
    let handle = start_api_server(state.clone(), port);
    *state.proxy_handle.lock() = Some(handle);

    // 构建 Tauri 桌面应用
    tauri::Builder::default()
        // 注册 Tauri 插件
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // 注入应用全局状态
        .manage(state)
        // 注册 Tauri 命令处理器
        .invoke_handler(tauri::generate_handler![
            tauri_cmds::provider_cmds::list_providers,
            tauri_cmds::provider_cmds::add_provider,
            tauri_cmds::provider_cmds::test_provider,
            tauri_cmds::settings_cmds::get_settings,
            tauri_cmds::settings_cmds::update_settings,
            tauri_cmds::proxy_cmds::start_proxy,
            tauri_cmds::proxy_cmds::stop_proxy,
            tauri_cmds::status_cmds::get_system_status,
            tauri_cmds::chat_cmds::chat_completions_stream,
            tauri_cmds::chat_cmds::cancel_chat_stream,
        ])
        // 应用初始化回调：创建系统托盘
        .setup(|app| {
            use tauri::Emitter;
            use tauri::Manager;
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri::WindowEvent;

            // 状态面板窗口失焦时自动隐藏
            if let Some(panel) = app.get_webview_window("status-panel") {
                let panel_clone = panel.clone();
                panel.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = panel_clone.hide();
                    }
                });
            }

            // 构建托盘菜单项
            let show = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let start_item = MenuItemBuilder::with_id("start_proxy", "启动代理").build(app)?;
            let stop_item = MenuItemBuilder::with_id("stop_proxy", "停止代理").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            // 组装托盘菜单
            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&start_item)
                .item(&stop_item)
                .item(&quit)
                .build()?;

            // 构建系统托盘图标
            let mut builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Vortex AI Gateway")
                .show_menu_on_left_click(false)
                // 托盘菜单事件处理
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        // 显示并聚焦主窗口
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "start_proxy" => {
                        // 向前端发送启动代理事件
                        let _ = app.emit("tray-action", "start");
                    }
                    "stop_proxy" => {
                        // 向前端发送停止代理事件
                        let _ = app.emit("tray-action", "stop");
                    }
                    _ => {}
                })
                // 托盘图标点击事件处理
                .on_tray_icon_event(|tray, event| {
                    // 左键点击直接显示并聚焦主窗口
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                });

            // 设置托盘图标：单色模板图（黑剪影 + 透明背景）
            // macOS 菜单栏约定：template 模式下系统只取 alpha 通道，
            // 浅色栏渲染为黑色、深色栏渲染为白色，与应用原生托盘观感一致
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))
                .expect("failed to load tray template icon");
            builder = builder
                .icon(tray_icon)
                .icon_as_template(true);

            // 构建托盘图标
            let _tray = builder.build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
