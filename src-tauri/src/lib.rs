//! Vortex AI Gateway 桌面应用入口。
//!
//! 负责加载配置、创建应用状态、启动 HTTP API 服务器，
//! 并构建 Tauri 桌面应用（含系统托盘、窗口管理、IPC 命令注册）。

mod tauri_cmds;

use std::sync::Arc;
use vortex_gateway::AppState;

/// 应用主入口函数。
///
/// 执行完整的启动流程：
/// 1. 从环境变量加载应用配置
/// 2. 创建应用全局状态（初始化数据库、HTTP 客户端等）
/// 3. 启动内嵌 HTTP API 服务器
/// 4. 启动 Tauri 桌面应用，注册插件、命令处理器与系统托盘
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();

    let cfg = vortex_store::config::AppConfig::load();
    let state = vortex_gateway::create_app_state(cfg.clone()).expect("Failed to initialize app state");

    let port = cfg.port;
    let handle = vortex_gateway::start_api_server(state.clone(), port);
    *state.proxy_handle.lock() = Some(handle);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state)
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
            tauri_cmds::agent_cmds::agent_detect,
            tauri_cmds::agent_cmds::agent_preview_config,
            tauri_cmds::agent_cmds::agent_apply_config,
            tauri_cmds::agent_cmds::agent_restore_config,
            tauri_cmds::agent_cmds::agent_list_backups,
        ])
        .setup(|app| {
            use tauri::Emitter;
            use tauri::Manager;
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri::WindowEvent;

            if let Some(panel) = app.get_webview_window("status-panel") {
                let panel_clone = panel.clone();
                panel.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = panel_clone.hide();
                    }
                });
            }

            #[cfg(unix)]
            {
                let signal_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sigint = match signal(SignalKind::interrupt()) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut sigterm = match signal(SignalKind::terminate()) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut sighup = match signal(SignalKind::hangup()) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    tokio::select! {
                        _ = sigint.recv() => {},
                        _ = sigterm.recv() => {},
                        _ = sighup.recv() => {},
                    }
                    log::info!("收到终止信号，退出应用");
                    signal_handle.exit(0);
                });
            }

            if let Some(main) = app.get_webview_window("main") {
                let main_clone = main.clone();
                let app_handle = app.handle().clone();
                main.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_clone.hide();
                        #[cfg(target_os = "macos")]
                        let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                });
            }

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
            let menu_cell = std::sync::Mutex::new(Some(menu));
            let start_item_for_menu = start_item.clone();
            let stop_item_for_menu = stop_item.clone();

            let mut builder = TrayIconBuilder::new()
                .tooltip("Vortex AI Gateway")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        #[cfg(target_os = "macos")]
                        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "start_proxy" => {
                        let app_handle = app.clone();
                        let si = start_item_for_menu.clone();
                        let spi = stop_item_for_menu.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<Arc<AppState>>();
                            let mut guard = state.proxy_handle.lock();
                            if guard.is_none() {
                                let handle = vortex_gateway::start_api_server(state.inner().clone(), state.proxy_port);
                                *guard = Some(handle);
                                let _ = si.set_enabled(false);
                                let _ = spi.set_enabled(true);
                                let _ = app_handle.emit("tray-action", "started");
                            }
                        });
                    }
                    "stop_proxy" => {
                        let app_handle = app.clone();
                        let si = start_item_for_menu.clone();
                        let spi = stop_item_for_menu.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<Arc<AppState>>();
                            let handle = { state.proxy_handle.lock().take() };
                            if let Some(handle) = handle {
                                handle.stop(true).await;
                                let _ = si.set_enabled(true);
                                let _ = spi.set_enabled(false);
                                let _ = app_handle.emit("tray-action", "stopped");
                            }
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            let app = tray.app_handle();
                            #[cfg(target_os = "macos")]
                            let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        TrayIconEvent::Click {
                            button: MouseButton::Right,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            if let Some(menu) = menu_cell.lock().unwrap().take() {
                                let _ = tray.set_menu(Some(menu));
                                let _ = tray.set_show_menu_on_left_click(false);
                            }
                            let app = tray.app_handle();
                            let state = app.state::<Arc<AppState>>();
                            let is_running = state.proxy_handle.lock().is_some();
                            let _ = start_item.set_enabled(!is_running);
                            let _ = stop_item.set_enabled(is_running);
                        }
                        _ => {}
                    }
                });

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))
                .expect("failed to load tray template icon");
            builder = builder
                .icon(tray_icon)
                .icon_as_template(true);

            let _tray = builder.build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
