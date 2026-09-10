mod db;
mod error;
mod config;
mod providers;
mod routing;
mod proxy;
mod translator;
mod api;
mod tauri_cmds;

use actix_cors::Cors;
use actix_web::{dev::ServerHandle, web, App, HttpServer, middleware as actix_mw};
use std::sync::{mpsc, Arc};

pub struct AppState {
    pub db_pool: db::core::DbPool,
    pub config: config::AppConfig,
    pub provider_registry: providers::ProviderRegistry,
    pub proxy_engine: parking_lot::RwLock<proxy::engine::ProxyEngine>,
    pub resilience_manager: routing::resilience::ResilienceManager,
    pub http_client: reqwest::Client,
    pub encryption_key: Vec<u8>,
    pub proxy_handle: parking_lot::Mutex<Option<ServerHandle>>,
    pub proxy_port: u16,
}

pub fn create_app_state(cfg: config::AppConfig) -> error::Result<Arc<AppState>> {
    let db_pool = db::core::init_pool(&cfg.data_dir, cfg.encryption_key.clone())?;
    db::core::run_migrations(&db_pool)?;

    let provider_registry = providers::ProviderRegistry::new();
    let proxy_engine = proxy::engine::ProxyEngine::new();
    let resilience_manager = routing::resilience::ResilienceManager::new();
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(10))
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| error::AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let encryption_key = cfg.encryption_key_bytes();

    let proxy_port = cfg.port;

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
                            .route("/models", web::get().to(api::v1::models::list_models))
                            .route("/embeddings", web::post().to(api::v1::embeddings::create_embeddings))
                            .route("/images/generations", web::post().to(api::v1::images::create_images))
                    )
                    .service(
                        web::scope("/api")
                            .route("/providers", web::get().to(api::management::providers::list_providers))
                            .route("/providers", web::post().to(api::management::providers::create_provider))
                            .route("/providers/{id}", web::get().to(api::management::providers::get_provider))
                            .route("/providers/{id}", web::patch().to(api::management::providers::update_provider))
                            .route("/providers/{id}", web::delete().to(api::management::providers::delete_provider))
                            .route("/providers/{id}/test", web::post().to(api::management::providers::test_provider))
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
                            .route("/health", web::get().to(api::management::health::health_check))
                    )
            })
            .bind(format!("0.0.0.0:{}", port))
            .expect("Failed to bind server");

            let srv = server.run();
            let handle = srv.handle();
            let _ = handle_tx.send(handle);
            srv.await.expect("Server error");
        });
    });

    handle_rx.recv().expect("Failed to receive server handle")
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::AppConfig::load();
    let state = create_app_state(cfg.clone()).expect("Failed to initialize app state");

    let port = cfg.port;
    let handle = start_api_server(state.clone(), port);
    *state.proxy_handle.lock() = Some(handle);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            tauri_cmds::provider_cmds::list_providers,
            tauri_cmds::provider_cmds::add_provider,
            tauri_cmds::provider_cmds::test_provider,
            tauri_cmds::settings_cmds::get_settings,
            tauri_cmds::settings_cmds::update_settings,
            tauri_cmds::proxy_cmds::start_proxy,
            tauri_cmds::proxy_cmds::stop_proxy,
        ])
        .setup(|app| {
            use tauri::Emitter;
            use tauri::Manager;
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let show = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let start_item = MenuItemBuilder::with_id("start_proxy", "启动代理").build(app)?;
            let stop_item = MenuItemBuilder::with_id("stop_proxy", "停止代理").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&start_item)
                .item(&stop_item)
                .item(&quit)
                .build()?;

            let mut builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Vortex AI Gateway")
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "start_proxy" => {
                        let _ = app.emit("tray-action", "start");
                    }
                    "stop_proxy" => {
                        let _ = app.emit("tray-action", "stop");
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                builder = builder.icon(icon);
            }

            let _tray = builder.build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
