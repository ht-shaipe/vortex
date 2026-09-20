//! Vortex Gateway 独立 HTTP 服务器入口。
//!
//! 不依赖 Tauri，纯 actix-web 服务器，供 Web 模式部署使用。
//!
//! ## 启动方式
//!
//! ```sh
//! # 直接运行
//! cargo run -p vortex-gateway --bin vortex-server
//!
//! # 或编译后运行
//! cargo build -p vortex-gateway --bin vortex-server
//! ./target/debug/vortex-server
//! ```
//!
//! ## 环境变量
//!
//! 与桌面端共享同一套环境变量配置（见 [`vortex_store::config::AppConfig::load`]），
//! 常用的有：
//!
//! | 变量 | 说明 | 默认值 |
//! |------|------|--------|
//! | `VORTEX_PORT` | 监听端口 | `10168` |
//! | `VORTEX_DATA_DIR` | 数据目录 | 系统数据目录下 `vortex` |
//! | `VORTEX_LOG_LEVEL` | 日志级别 | `info` |
//!
//! ## 前端连接
//!
//! 前端通过 `runtime.kind === 'web'` 检测到非桌面环境后，
//! 自动走 HTTP API（`http://localhost:10168/api/*`）而非 Tauri IPC。

use std::sync::Arc;

use vortex_gateway::{create_app_state, start_api_server, AppState};
use vortex_store::config::AppConfig;

fn main() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .try_init();

    let cfg = AppConfig::load();
    let port = cfg.port;

    log::info!("Vortex Gateway 启动中 — 端口 {port}");

    let state: Arc<AppState> =
        create_app_state(cfg).expect("Failed to initialize app state");

    let handle = start_api_server(state.clone(), port);
    *state.proxy_handle.lock() = Some(handle);

    log::info!("Vortex Gateway 已启动 — 监听 0.0.0.0:{port}");
    log::info!("API 端点: http://localhost:{port}/api/*");
    log::info!("代理端点: http://localhost:{port}/v1/*");
    log::info!("按 Ctrl+C 停止服务器");

    // 主线程等待 Ctrl+C / SIGTERM
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigint = signal(SignalKind::interrupt()).expect("Failed to bind SIGINT");
                let mut sigterm = signal(SignalKind::terminate()).expect("Failed to bind SIGTERM");
                tokio::select! {
                    _ = sigint.recv() => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            let _ = tx.send(());
        });
    });
    let _ = rx.recv();

    log::info!("正在停止服务器...");

    // 优雅关闭：在 actix runtime 中 await handle.stop()
    let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
    rt.block_on(async {
        if let Some(handle) = state.proxy_handle.lock().take() {
            handle.stop(true).await;
        }
    });

    log::info!("服务器已停止");
}
