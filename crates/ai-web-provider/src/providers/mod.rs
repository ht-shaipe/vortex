//! 平台 provider：每个平台实现 Provider trait，在独立 OS 线程的
//! current_thread runtime + LocalSet 中运行（obscura 的 V8 !Send）。

pub mod chatgpt;
pub mod claude;
pub mod deepseek;
pub mod doubao;
pub mod kimi;
pub mod qianwen;

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::config::{CommonCfg, ProviderCfg};
use crate::models::*;
use crate::providers::chatgpt::ChatgptProvider;
use crate::providers::claude::ClaudeProvider;
use crate::providers::deepseek::DeepseekProvider;
use crate::providers::doubao::DoubaoProvider;
use crate::providers::kimi::KimiProvider;
use crate::providers::qianwen::QianwenProvider;

/// 平台 provider 抽象。静态泛型分发（不 dyn），规避 V8 !Send 的动态分发问题。
pub trait Provider: 'static {
    /// 平台标识（小写，用于 URL 路径与存储子目录）
    const NAME: &'static str;
    /// 平台显示名（豆包 / DeepSeek / ...）
    const DISPLAY: &'static str;
    /// 默认站点地址
    const DEFAULT_URL: &'static str;
    /// 覆盖站点地址的环境变量名（如 DOUBAO_URL）
    const URL_ENV: &'static str;

    /// 在 worker 线程的 LocalSet 内运行：构建自有引擎资源 + 命令循环，
    /// 直到命令通道关闭。provider 全权负责自身引擎生命周期。
    fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) -> impl Future<Output = ()> + 'static;
}

/// 按平台名启动 worker 线程，注册进 workers 表。
/// 返回 false 表示平台名未知（不启动）。
pub fn spawn_platform(
    name: &str,
    common: &CommonCfg,
    workers: &mut HashMap<String, mpsc::Sender<Command>>,
) -> bool {
    let tx = match name {
        "chatgpt" => spawn_worker_thread::<ChatgptProvider>(common),
        "claude" => spawn_worker_thread::<ClaudeProvider>(common),
        "doubao" => spawn_worker_thread::<DoubaoProvider>(common),
        "deepseek" => spawn_worker_thread::<DeepseekProvider>(common),
        "qianwen" => spawn_worker_thread::<QianwenProvider>(common),
        "kimi" => spawn_worker_thread::<KimiProvider>(common),
        _ => return false,
    };
    workers.insert(name.to_string(), tx);
    true
}

/// 为平台 P 启动独立 OS 线程（current_thread runtime + LocalSet），
/// 返回该平台命令通道的发送端。
fn spawn_worker_thread<P: Provider>(common: &CommonCfg) -> mpsc::Sender<Command> {
    let cfg = Arc::new(ProviderCfg::for_platform(
        common,
        P::NAME,
        P::DISPLAY,
        P::DEFAULT_URL,
        P::URL_ENV,
    ));
    let (tx, rx) = mpsc::channel::<Command>(16);

    // cookie 持久化：obscura 启动自动加载该文件；provider 登录成功/周期/退出时写入。
    // 重启后 cookie 未过期即自动恢复登录态，无需重新扫码。
    if let Err(e) = std::fs::create_dir_all(&cfg.storage_dir) {
        tracing::error!("[{}] 创建存储目录 {} 失败: {e}", P::NAME, cfg.storage_dir);
    }

    let thread_name = format!("obscura-{}", P::NAME);
    std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            // obscura 的 Browser / Page 内部持有 V8（单线程、!Send），
            // 不能在 actix 的多线程 runtime 上 tokio::spawn。
            // 放到独立 OS 线程的 current-thread runtime + LocalSet 中运行。
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("[{}] 构建 worker runtime 失败: {e}", P::NAME);
                    return;
                }
            };
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, P::run(cfg, rx));
        })
        .unwrap_or_else(|e| panic!("spawn {thread_name} 失败: {e}"));
    tx
}

/// 尚未接入的平台（Phase 2-4 实现前）的占位命令处理：
/// Status 如实返回未登录，其余命令返回「尚未接入」错误。
#[allow(dead_code)]
pub(crate) async fn handle_stub_command(name: &str, phase: &str, cmd: Command) {
    let not_impl = || anyhow::anyhow!("{name} 平台尚未接入（规划 {phase} 实现），当前仅 doubao 平台可用");
    match cmd {
        Command::Status { reply_tx } => {
            let _ = reply_tx.send(StatusResponse {
                logged_in: false,
                qr_state: "none".to_string(),
            });
        }
        Command::Ask { reply_tx, .. } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::LoginQr { reply_tx } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::LoginPhone { reply_tx, .. } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::LoginVerify { reply_tx, .. } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::Reset { reply_tx } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::Debug { reply_tx } => {
            let _ = reply_tx.send(format!(
                "{{\"error\":\"{name} 平台尚未接入（规划 {phase} 实现）\"}}"
            ));
        }
        Command::DumpHtml { reply_tx, .. } => {
            let _ = reply_tx.send(String::new());
        }
        Command::SetDebugHtml { reply_tx, .. } => {
            let _ = reply_tx.send(DebugHtmlResponse { debug_html: false });
        }
        Command::Screenshot { reply_tx } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::ClickAt { reply_tx, .. } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
        Command::EvalJs { reply_tx, .. } => {
            let _ = reply_tx.send("null".into());
        }
        Command::ImportCookies { reply_tx, .. } => {
            let _ = reply_tx.send(Err(not_impl()));
        }
    }
}
