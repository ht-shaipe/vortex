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
use actix_web::{web, App, HttpServer, middleware as actix_mw};
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct AppState {
    pub db_pool: db::core::DbPool,
    pub config: config::AppConfig,
    pub provider_registry: providers::ProviderRegistry,
    pub routing_engine: routing::RoutingEngine,
    pub proxy_engine: parking_lot::RwLock<proxy::engine::ProxyEngine>,
    pub resilience_manager: routing::resilience::ResilienceManager,
    pub http_client: reqwest::Client,
    pub encryption_key: Vec<u8>,
}

pub fn create_app_state(cfg: config::AppConfig) -> error::Result<Arc<AppState>> {
    let db_pool = db::core::init_pool(&cfg.data_dir, cfg.encryption_key.clone())?;
    db::core::run_migrations(&db_pool)?;

    let provider_registry = providers::ProviderRegistry::new();
    let routing_engine = routing::RoutingEngine::new();
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

    Ok(Arc::new(AppState {
        db_pool,
        config: cfg,
        provider_registry,
        routing_engine,
        proxy_engine: parking_lot::RwLock::new(proxy_engine),
        resilience_manager,
        http_client,
        encryption_key,
    }))
}

pub fn start_api_server(
    state: Arc<AppState>,
    port: u16,
) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();

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
                            .route("/combos", web::get().to(api::management::combos::list_combos))
                            .route("/combos", web::post().to(api::management::combos::create_combo))
                            .route("/combos/{id}", web::get().to(api::management::combos::get_combo))
                            .route("/combos/{id}", web::patch().to(api::management::combos::update_combo))
                            .route("/combos/{id}", web::delete().to(api::management::combos::delete_combo))
                            .route("/keys", web::get().to(api::management::keys::list_keys))
                            .route("/keys", web::post().to(api::management::keys::create_key))
                            .route("/keys/{id}", web::get().to(api::management::keys::get_key))
                            .route("/keys/{id}", web::delete().to(api::management::keys::delete_key))
                            .route("/usage", web::get().to(api::management::usage::get_usage))
                            .route("/usage/stats", web::get().to(api::management::usage::get_stats))
                            .route("/settings", web::get().to(api::management::settings::get_settings))
                            .route("/settings", web::patch().to(api::management::settings::update_settings))
                            .route("/health", web::get().to(api::management::health::health_check))
                    )
            })
            .bind(format!("0.0.0.0:{}", port))
            .expect("Failed to bind server");

            let _ = tx.send(());
            server.run().await.expect("Server error");
        });
    });

    rx
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::AppConfig::load();
    let state = create_app_state(cfg.clone()).expect("Failed to initialize app state");

    let port = cfg.port;
    start_api_server(state.clone(), port);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            tauri_cmds::provider_cmds::list_providers,
            tauri_cmds::provider_cmds::add_provider,
            tauri_cmds::provider_cmds::test_provider,
            tauri_cmds::combo_cmds::list_combos,
            tauri_cmds::combo_cmds::save_combo,
            tauri_cmds::combo_cmds::delete_combo,
            tauri_cmds::settings_cmds::get_settings,
            tauri_cmds::settings_cmds::update_settings,
        ])
        .setup(|app| {
            use tauri::Manager;
            use tauri::tray::TrayIconBuilder;
            use tauri::menu::{MenuBuilder, MenuItemBuilder};

            let show_item = MenuItemBuilder::with_id("show", "Open Dashboard").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Vortex AI Gateway")
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "quit" => app.exit(0),
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
