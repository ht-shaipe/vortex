//! 网页版平台（浏览器自动化）服务。
//!
//! 持有 ai-web-provider 的平台 worker 集合（每平台一个 obscura 引擎线程，
//! V8 !Send），实现 [`WebChatBridge`] 供代理引擎的 WebExecutor 调用；
//! 同时内置 [`CdpLoginManager`]，编排「拉起本地 Chrome → 用户登录/使用 →
//! CDP 持续同步凭证（含聊天产生的 x5sec/userToken 轮换）→ 注入 worker →
//! 校验持久化」的授权流程。ask 遇到未登录错误时会自动接管已打开的
//! 授权窗口（或拉起新窗口），等待凭证收集完成后重试原问题；流式路径
//! 通过 `Status` chunk 向聊天界面推送当前进展。
//!
//! 存储布局：`<data_dir>/web-providers/<platform>/`（cookie 持久化于其中，
//! 重启自动恢复登录态）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ai_web_provider::cdp_auth::CdpLoginManager;
use ai_web_provider::common::chunk::{decode_chunk, ChunkKind};
use ai_web_provider::config::CommonCfg;
use ai_web_provider::models::{Command, ERR_NOT_LOGGED_IN};
use ai_web_provider::providers::spawn_platform;
use futures::SinkExt;
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};
use vortex_router::proxy::web_bridge::{
    WebChatBridge, WebChunk, WebChunkKind, WebReply, WebStreamHandle,
};

/// 登录恢复的最长等待时间（与 CDP 授权窗口上限一致）。
/// 等待期间流式通道会持续推送进度提示，避免前端误以为请求卡死。
const LOGIN_RECOVERY_TIMEOUT: Duration = Duration::from_secs(600);

/// 登录恢复的状态轮询间隔
const LOGIN_POLL_INTERVAL: Duration = Duration::from_millis(800);

/// 主动触发凭证收集的重试间隔。自动同步在校验失败后会记录指纹并
/// 停止重试，用户刚在浏览器里登录/聊天时需要一次手动收集兜底。
const LOGIN_FORCE_RETRY: Duration = Duration::from_secs(8);

/// 新拉起的授权窗口先留给自动检测的宽限期。避免用户还在登录时
/// 就用 profile 里的旧凭证强制收集，导致窗口被提前关闭。
const LOGIN_FORCE_GRACE: Duration = Duration::from_secs(30);

/// 已打开的会话至少运行这么久，才把「存在凭证」视为用户已完成登录
/// 的信号并主动收集；刚打开的窗口可能只是 profile 残留。
const LOGIN_FORCE_MIN_AGE_SECS: u64 = 10;

/// 平台默认站点地址（与 ai-web-provider 各 Provider 的 DEFAULT_URL 对齐）
pub fn platform_default_urls() -> HashMap<String, String> {
    [
        ("chatgpt", "https://chatgpt.com/"),
        ("claude", "https://claude.ai/"),
        ("doubao", "https://www.doubao.com/chat/"),
        ("deepseek", "https://chat.deepseek.com/"),
        ("qianwen", "https://www.qianwen.com/chat"),
        ("kimi", "https://www.kimi.com/"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

/// 网页版平台桥接：worker 集合 + CDP 授权管理。
pub struct WebProviderBridge {
    /// 平台 → worker 命令通道发送端
    workers: RwLock<HashMap<String, mpsc::Sender<Command>>>,
    /// CDP 授权会话管理器（拉起本地 Chrome 登录）。
    /// Arc：ask 失败自动重授权路径要把它 move 进异步任务
    pub cdp: Arc<CdpLoginManager>,
    /// 平台默认站点地址
    base_urls: HashMap<String, String>,
    /// worker 公共配置（按需启动新 worker 时复用）
    common: CommonCfg,
}

impl WebProviderBridge {
    /// 启动指定平台集合的 worker 线程。
    ///
    /// - `data_dir`：vortex 数据目录（web 平台存储根：`{data_dir}/web-providers`）
    /// - `platforms_csv`：逗号分隔的平台名（doubao/chatgpt/claude/deepseek/qianwen/kimi）；
    ///   未列出的已知平台可后续通过 [`Self::ensure_platform`] 按需启动
    ///
    /// 除显式列表外，磁盘上已保存登录凭据（cookies.json）的已知平台
    /// 会在启动时自动拉起 worker——重启网关后已授权平台无需手动干预。
    pub fn new(data_dir: &str, platforms_csv: &str) -> Self {
        let mut common = CommonCfg::from_env();
        common.storage_base = format!("{}/web-providers", data_dir.trim_end_matches('/'));
        // HTML trace 会往工作目录写文件，网关内默认关闭（可用 DEBUG_HTML=1 强制）
        common.debug_html = std::env::var("DEBUG_HTML")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let base_urls = platform_default_urls();
        let mut workers = HashMap::new();
        for name in platforms_csv.split(',') {
            let name = name.trim().to_lowercase();
            if name.is_empty() {
                continue;
            }
            if spawn_platform(&name, &common, &mut workers) {
                log::info!(
                    "[web] 平台 {name} worker 已启动（storage: {}）",
                    common.storage_base
                );
            } else {
                log::warn!(
                    "[web] 未知平台 {name}，跳过（支持: doubao/chatgpt/claude/deepseek/qianwen/kimi）"
                );
            }
        }
        // 已授权平台自动拉起：扫描存储目录，凡是有 cookies.json 的已知平台
        // （且尚未启动）直接启动 worker，恢复会话供别名/直连路由使用
        if let Ok(entries) = std::fs::read_dir(&common.storage_base) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().trim().to_lowercase();
                if !base_urls.contains_key(&name) || workers.contains_key(&name) {
                    continue;
                }
                if !entry.path().join("cookies.json").is_file() {
                    continue;
                }
                if spawn_platform(&name, &common, &mut workers) {
                    log::info!("[web] 检测到已授权平台 {name}，worker 自动启动");
                }
            }
        }
        Self {
            workers: RwLock::new(workers),
            cdp: Arc::new(CdpLoginManager::new()),
            base_urls,
            common,
        }
    }

    /// 已启动的平台列表
    pub fn platforms(&self) -> Vec<String> {
        self.workers.read().keys().cloned().collect()
    }

    /// 全部已知平台（含未启动的），按名称排序保证列表稳定
    pub fn known_platforms(&self) -> Vec<String> {
        let mut names: Vec<String> = self.base_urls.keys().cloned().collect();
        names.sort();
        names
    }

    /// 按需启动指定平台的 worker（已启动则幂等返回）。
    /// 供「网页版账号」页选择平台授权时调用。
    pub fn ensure_platform(&self, platform: &str) -> Result<(), String> {
        if !self.base_urls.contains_key(platform) {
            return Err(format!(
                "未知网页版平台: {platform}（支持: doubao/chatgpt/claude/deepseek/qianwen/kimi）"
            ));
        }
        if self.workers.read().contains_key(platform) {
            return Ok(());
        }
        let mut workers = self.workers.write();
        // 双检：并发调用时后到者直接幂等返回
        if workers.contains_key(platform) {
            return Ok(());
        }
        if spawn_platform(platform, &self.common, &mut workers) {
            log::info!("[web] 平台 {platform} worker 已按需启动");
            Ok(())
        } else {
            Err(format!("平台 {platform} worker 启动失败"))
        }
    }

    fn worker_tx(&self, platform: &str) -> Result<mpsc::Sender<Command>, String> {
        self.workers.read().get(platform).cloned().ok_or_else(|| {
            format!(
                "网页版平台 {platform} 的 worker 未启动（可用: {:?}）",
                self.platforms()
            )
        })
    }

    /// 拉起本地 Chrome 开始 CDP 授权（在浏览器中完成平台登录）。
    /// 平台 worker 未启动时按需启动（「网页版账号」页选择平台即授权）。
    pub async fn start_cdp_login(&self, platform: &str) -> Result<(), String> {
        let base_url = self
            .base_urls
            .get(platform)
            .cloned()
            .ok_or_else(|| format!("未知网页版平台: {platform}"))?;
        self.ensure_platform(platform)?;
        let tx = self.worker_tx(platform)?;
        self.cdp
            .start(platform, &base_url, tx)
            .await
            .map_err(|e| e.to_string())
    }

    /// CDP 授权状态
    pub async fn cdp_status(
        &self,
        platform: &str,
    ) -> Option<ai_web_provider::cdp_auth::CdpLoginStatus> {
        self.cdp.status(platform).await
    }

    /// 手动触发 CDP cookie 收集（用户确认已登录）
    pub async fn cdp_finish(&self, platform: &str) -> bool {
        self.cdp.finish(platform).await
    }

    /// 取消 CDP 授权（杀 Chrome）
    pub async fn cdp_cancel(&self, platform: &str) -> bool {
        self.cdp.cancel(platform).await
    }

    /// 查询平台登录态（转 Status 命令给 worker）
    pub async fn login_status(&self, platform: &str) -> Result<bool, String> {
        let tx = self.worker_tx(platform)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(Command::Status { reply_tx })
            .await
            .map_err(|_| "worker 已退出".to_string())?;
        reply_rx
            .await
            .map(|s| s.logged_in)
            .map_err(|_| "worker 已退出".to_string())
    }

    /// 平台运行时诊断（URL / 登录态 / 最后回复等，Debug 命令）
    pub async fn platform_debug(&self, platform: &str) -> Result<String, String> {
        let tx = self.worker_tx(platform)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(Command::Debug { reply_tx })
            .await
            .map_err(|_| "worker 已退出".to_string())?;
        reply_rx
            .await
            .map_err(|_| "worker 未响应 debug".to_string())
    }

    /// 构造登录恢复上下文（'static，可 move 进流式任务）。
    fn login_recovery(&self, platform: &str) -> Result<LoginRecovery, String> {
        let base_url = self
            .base_urls
            .get(platform)
            .cloned()
            .ok_or_else(|| format!("未知网页版平台: {platform}"))?;
        let worker_tx = self.worker_tx(platform)?;
        Ok(LoginRecovery {
            platform: platform.to_string(),
            base_url,
            worker_tx,
            cdp: self.cdp.clone(),
        })
    }

    /// 重置平台页面（新开 page 并导航回站点，丢弹窗/风控遮罩）
    pub async fn platform_reset(&self, platform: &str) -> Result<(), String> {
        let tx = self.worker_tx(platform)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(Command::Reset { reply_tx })
            .await
            .map_err(|_| "worker 已退出".to_string())?;
        match reply_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("worker 未响应 reset".to_string()),
        }
    }
}

/// worker ask 错误分类：登录态失效单独识别，便于触发 CDP 凭证恢复。
enum WorkerAskError {
    /// `ERR_NOT_LOGGED_IN` 前缀已剥离，保留 provider 的可读提示
    LoginRequired(String),
    /// 其他上游错误（原样返回）
    Other(String),
}

/// 识别并剥离内部未登录前缀，避免泄漏给最终客户端。
fn classify_worker_error(msg: String) -> WorkerAskError {
    match msg.strip_prefix(ERR_NOT_LOGGED_IN) {
        Some(rest) => WorkerAskError::LoginRequired(rest.trim().to_string()),
        None => WorkerAskError::Other(msg),
    }
}

/// 登录恢复过程中向前端推送的状态（仅流式路径有）。
type ProgressSink = Option<futures::channel::mpsc::Sender<WebChunk>>;

/// 向前端发送一条过程提示（不进入正文，不参与 token 统计）。
async fn push_status(
    tx: Option<&mut futures::channel::mpsc::Sender<WebChunk>>,
    text: impl Into<String>,
) {
    if let Some(tx) = tx {
        let _ = tx
            .send(WebChunk {
                kind: WebChunkKind::Status,
                text: text.into(),
            })
            .await;
    }
}

/// ask 失败后的登录恢复上下文（'static，可 move 进流式任务）。
struct LoginRecovery {
    platform: String,
    base_url: String,
    worker_tx: mpsc::Sender<Command>,
    cdp: Arc<CdpLoginManager>,
}

impl LoginRecovery {
    /// 确保 CDP 授权窗口存在并完成凭证收集。
    ///
    /// - 已有进行中的会话：等待登录；检测到凭证后主动触发一次 `finish`
    ///   （自动同步可能因指纹未变化而停下，用户刚刚在浏览器里操作过时
    ///   手动收集最可靠）；
    /// - 无会话 / 上次失败或取消：重新拉起 Chrome；
    /// - 已成功：凭证此前已注入，直接返回让调用方重试 ask。
    ///
    /// 等待期间通过 `progress` 推送「正在收集」等提示，收集成功后返回
    /// Ok，由调用方重新发送原问题。
    async fn ensure(&self, progress: &mut ProgressSink) -> Result<(), String> {
        log::info!(
            "[web] {} 登录态失效，开始自动恢复（复用已打开的 CDP 窗口或拉起新窗口）",
            self.platform
        );
        let started_at = Instant::now();
        let mut started = false;
        // 最近一次手动触发收集的时间（节流 + 失败后允许重试）
        let mut last_force: Option<Instant> = None;

        match self.cdp.status(&self.platform).await {
            Some(st) if st.phase == "success" => {
                push_status(progress.as_mut(), "检测到已同步的登录凭证，正在重试发送…").await;
                return Ok(());
            }
            Some(st) if st.phase == "waiting_login" || st.phase == "importing" => {
                if st.phase == "waiting_login" && st.has_credentials {
                    if st.elapsed_secs >= LOGIN_FORCE_MIN_AGE_SECS {
                        push_status(progress.as_mut(), "检测到登录凭证，正在收集并注入…").await;
                        if self.cdp.finish(&self.platform).await {
                            last_force = Some(Instant::now());
                        }
                    } else {
                        push_status(
                            progress.as_mut(),
                            "检测到登录凭证，正在等待页面稳定后自动收集…",
                        )
                        .await;
                    }
                } else if st.phase == "importing" {
                    // 已在导入中：记录时间，失败回到 waiting 后按间隔重试
                    last_force = Some(Instant::now());
                    push_status(progress.as_mut(), "正在收集登录凭证并注入网关…").await;
                } else {
                    push_status(progress.as_mut(), "等待浏览器完成登录，登录后将自动发送…").await;
                }
            }
            Some(st) => {
                // failed / cancelled：上一次窗口已结束，重新拉起
                log::info!(
                    "[web] {} 上次 CDP 会话状态为 {}，重新拉起授权",
                    self.platform,
                    st.phase
                );
                self.start(progress).await?;
                started = true;
            }
            None => {
                self.start(progress).await?;
                started = true;
            }
        }

        self.wait_ready(progress, started, started_at, last_force)
            .await
    }

    /// 已同步过凭证但仍无法通过 worker 校验（典型：页面后来被登出、
    /// 旧 token 失效）：关闭旧窗口后重新拉起一次，等待新的登录/同步。
    async fn restart(&self, progress: &mut ProgressSink) -> Result<(), String> {
        log::info!(
            "[web] {} 已同步的凭证仍不可用，重启 CDP 授权窗口",
            self.platform
        );
        // 旧窗口可能仍占用 profile；先取消/关闭再拉起，避免启动失败
        let _ = self.cdp.cancel(&self.platform).await;
        push_status(progress.as_mut(), "当前凭证仍不可用，正在重新拉起 Chrome…").await;
        self.start(progress).await?;
        self.wait_ready(progress, true, Instant::now(), None).await
    }

    /// 等待 CDP 会话进入 success；期间处理登录提示与手动收集。
    async fn wait_ready(
        &self,
        progress: &mut ProgressSink,
        started: bool,
        started_at: Instant,
        mut last_force: Option<Instant>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + LOGIN_RECOVERY_TIMEOUT;
        let mut last_phase = String::new();
        loop {
            if Instant::now() >= deadline {
                return Err(format!(
                    "{} 登录等待超时（{} 分钟），请到「网页版账号」页检查授权状态后重试",
                    self.platform,
                    LOGIN_RECOVERY_TIMEOUT.as_secs() / 60
                ));
            }
            tokio::time::sleep(LOGIN_POLL_INTERVAL).await;

            let Some(st) = self.cdp.status(&self.platform).await else {
                return Err(format!(
                    "{} 的 CDP 授权会话已结束，请重新发起授权",
                    self.platform
                ));
            };

            if st.phase == "success" {
                log::info!("[web] {} 登录凭证已同步，重试原问题", self.platform);
                push_status(progress.as_mut(), "登录凭证已同步，正在重新发送消息…").await;
                return Ok(());
            }
            if st.phase == "failed" || st.phase == "cancelled" {
                return Err(format!("{} 授权未完成：{}", self.platform, st.message));
            }
            if st.phase != last_phase {
                match st.phase.as_str() {
                    "waiting_login" => {
                        let text = if started {
                            "Chrome 已打开，请在浏览器完成登录；登录后将自动发送…"
                        } else {
                            "等待浏览器完成登录，登录后将自动发送…"
                        };
                        push_status(progress.as_mut(), text).await;
                    }
                    "importing" => {
                        last_force = Some(Instant::now());
                        push_status(progress.as_mut(), "正在收集登录凭证并注入网关…").await;
                    }
                    _ => {}
                }
                last_phase = st.phase.clone();
            }

            // 已打开的会话：用户已经在浏览器里登录/使用过，但自动同步
            // 没有推进（例如指纹未变被去重）——主动触发一次收集。
            // 新拉起的会话先给自动检测一个窗口期，避免把 profile 里的
            // 旧凭证误当成刚完成的登录；导入失败回到 waiting 后，
            // 再按 LOGIN_FORCE_RETRY 间隔重试。
            let force_due = last_force
                .map(|t| t.elapsed() >= LOGIN_FORCE_RETRY)
                .unwrap_or(true);
            let force_allowed = if started {
                started_at.elapsed() >= LOGIN_FORCE_GRACE
            } else {
                st.elapsed_secs >= LOGIN_FORCE_MIN_AGE_SECS
            };
            if force_allowed
                && force_due
                && st.phase == "waiting_login"
                && st.has_credentials
                && self.cdp.finish(&self.platform).await
            {
                last_force = Some(Instant::now());
                push_status(progress.as_mut(), "检测到登录凭证，正在收集并注入…").await;
            }
        }
    }

    /// 拉起 CDP 授权窗口；并发场景下已有会话视为成功，直接接管等待。
    async fn start(&self, progress: &mut ProgressSink) -> Result<(), String> {
        push_status(progress.as_mut(), "登录态失效，正在拉起 Chrome 授权窗口…").await;
        match self
            .cdp
            .start(&self.platform, &self.base_url, self.worker_tx.clone())
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("已有进行中的 CDP 授权会话") {
                    // 另一个请求/授权页已经拉起，直接等待其完成
                    Ok(())
                } else {
                    Err(format!("无法拉起 Chrome 授权窗口：{msg}"))
                }
            }
        }
    }
}

/// 第二次 ask 仍报未登录时的可操作错误文案。
fn login_still_invalid(platform: &str, base: &str) -> String {
    format!(
        "{platform} 已尝试自动收集登录凭证，但网关仍报告未登录：{base}。\
         请到「网页版账号」页对 {platform} 重新授权后再试"
    )
}

/// 发送一次 ask 并等待完整回复。
async fn ask_once(tx: &mpsc::Sender<Command>, question: &str) -> Result<WebReply, WorkerAskError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::Ask {
        question: question.to_string(),
        chunk_tx: None,
        reply_tx,
    })
    .await
    .map_err(|_| WorkerAskError::Other("worker 已退出".to_string()))?;
    match reply_rx.await {
        Ok(Ok(r)) => Ok(to_web_reply(r)),
        Ok(Err(e)) => Err(classify_worker_error(e.to_string())),
        Err(_) => Err(WorkerAskError::Other("worker 已退出".to_string())),
    }
}

/// 发送一次流式 ask：把 worker 的编码 chunk 转发到 `out_tx`，
/// 返回最终完整回复。错误同样按登录态分类，供调用方决定是否恢复。
async fn ask_stream_once(
    tx: &mpsc::Sender<Command>,
    question: &str,
    out_tx: &mut futures::channel::mpsc::Sender<WebChunk>,
) -> Result<WebReply, WorkerAskError> {
    let (chunk_tx, mut chunk_rx) = mpsc::channel::<String>(64);
    let (reply_tx, mut reply_rx) = oneshot::channel();
    tx.send(Command::Ask {
        question: question.to_string(),
        chunk_tx: Some(chunk_tx),
        reply_tx,
    })
    .await
    .map_err(|_| WorkerAskError::Other("worker 已退出".to_string()))?;

    let reply = loop {
        tokio::select! {
            maybe = chunk_rx.recv() => match maybe {
                Some(raw) => forward_web_chunk(&raw, out_tx).await,
                None => {
                    break reply_rx
                        .await
                        .map_err(|_| WorkerAskError::Other("worker 已退出".to_string()))?;
                }
            },
            r = &mut reply_rx => {
                break r.map_err(|_| WorkerAskError::Other("worker 已退出".to_string()))?;
            }
        }
    };

    // worker 先发完 chunk 再回最终结果；把通道里剩余增量补发完，
    // 避免流式输出末尾被截断（客户端断开时发送失败可忽略）。
    while let Ok(raw) = chunk_rx.try_recv() {
        forward_web_chunk(&raw, out_tx).await;
    }

    match reply {
        Ok(r) => Ok(to_web_reply(r)),
        Err(e) => Err(classify_worker_error(e.to_string())),
    }
}

/// worker 编码 chunk（"T:xxx"/"H:xxx"）→ WebChunk 并转发。
async fn forward_web_chunk(raw: &str, out_tx: &mut futures::channel::mpsc::Sender<WebChunk>) {
    if raw.is_empty() {
        return;
    }
    let chunk = match decode_chunk(raw) {
        Some((ChunkKind::Thinking, t)) if !t.is_empty() => WebChunk {
            kind: WebChunkKind::Thinking,
            text: t,
        },
        Some((ChunkKind::Text, t)) if !t.is_empty() => WebChunk {
            kind: WebChunkKind::Text,
            text: t,
        },
        // 未编码的纯文本 chunk（旧实现）按正文处理
        _ => WebChunk {
            kind: WebChunkKind::Text,
            text: raw.to_string(),
        },
    };
    let _ = out_tx.send(chunk).await;
}

/// 把 AiReply 转换为 router 侧的 WebReply（丢弃搜索关键字/参考资料，
/// OpenAI 响应格式无标准承载字段）
fn to_web_reply(r: ai_web_provider::models::AiReply) -> WebReply {
    WebReply {
        thinking: r.thinking,
        reply: r.reply,
    }
}

#[async_trait::async_trait]
impl WebChatBridge for WebProviderBridge {
    fn available(&self, platform: &str) -> bool {
        self.workers.read().contains_key(platform)
    }

    async fn ask(&self, platform: &str, question: String) -> Result<WebReply, String> {
        // 聊天请求直接选中未启动的网页版平台时按需拉起 worker；
        // 若未授权，随后 ask 会返回未登录并进入 CDP 自动恢复流程。
        self.ensure_platform(platform)?;
        let tx = self.worker_tx(platform)?;
        match ask_once(&tx, &question).await {
            Ok(reply) => Ok(reply),
            Err(WorkerAskError::Other(e)) => Err(e),
            Err(WorkerAskError::LoginRequired(_)) => {
                // 登录态失效：拉起/接管 CDP 授权，等凭证注入后自动重试
                let recovery = self.login_recovery(platform)?;
                let mut progress: ProgressSink = None;
                recovery.ensure(&mut progress).await?;
                match ask_once(&tx, &question).await {
                    Ok(reply) => Ok(reply),
                    Err(WorkerAskError::Other(e)) => Err(e),
                    Err(WorkerAskError::LoginRequired(_)) => {
                        // 已有 success 会话但 worker 仍不认（旧 token 已失效）：
                        // 关闭旧窗口重新授权一次
                        recovery.restart(&mut progress).await?;
                        match ask_once(&tx, &question).await {
                            Ok(reply) => Ok(reply),
                            Err(WorkerAskError::Other(e)) => Err(e),
                            Err(WorkerAskError::LoginRequired(base)) => {
                                Err(login_still_invalid(platform, &base))
                            }
                        }
                    }
                }
            }
        }
    }

    async fn ask_stream(
        &self,
        platform: &str,
        question: String,
    ) -> Result<WebStreamHandle, String> {
        // 同 ask：聊天可直接拉起未启动的 worker（未授权时后续走 CDP 恢复）
        self.ensure_platform(platform)?;
        let recovery = self.login_recovery(platform)?;
        let tx = recovery.worker_tx.clone();
        let platform_owned = platform.to_string();

        // 立即返回流式句柄：登录恢复/重新发送都在后台任务里完成，
        // 期间通过 Status chunk 向前端推送进度，避免首字节超时。
        let (mut out_tx, out_rx) = futures::channel::mpsc::channel::<WebChunk>(64);
        let (result_tx, result_rx) = oneshot::channel::<Result<WebReply, String>>();
        tokio::spawn(async move {
            let mut progress: ProgressSink = Some(out_tx.clone());
            let result = match ask_stream_once(&tx, &question, &mut out_tx).await {
                Ok(reply) => Ok(reply),
                Err(WorkerAskError::Other(e)) => Err(e),
                Err(WorkerAskError::LoginRequired(_)) => {
                    push_status(
                        progress.as_mut(),
                        format!("{platform_owned} 登录态失效，正在自动恢复登录…"),
                    )
                    .await;
                    match recovery.ensure(&mut progress).await {
                        Ok(()) => match ask_stream_once(&tx, &question, &mut out_tx).await {
                            Ok(reply) => Ok(reply),
                            Err(WorkerAskError::Other(e)) => Err(e),
                            Err(WorkerAskError::LoginRequired(_)) => {
                                // 已有 success 会话但 worker 仍不认：
                                // 关闭旧窗口重新授权一次
                                push_status(
                                    progress.as_mut(),
                                    "已同步的凭证仍不可用，正在重新授权…",
                                )
                                .await;
                                match recovery.restart(&mut progress).await {
                                    Ok(()) => {
                                        match ask_stream_once(&tx, &question, &mut out_tx).await {
                                            Ok(reply) => Ok(reply),
                                            Err(WorkerAskError::Other(e)) => Err(e),
                                            Err(WorkerAskError::LoginRequired(base)) => {
                                                Err(login_still_invalid(&platform_owned, &base))
                                            }
                                        }
                                    }
                                    Err(e) => Err(e),
                                }
                            }
                        },
                        Err(e) => Err(e),
                    }
                }
            };
            let _ = result_tx.send(result);
        });

        let final_reply = async move {
            result_rx
                .await
                .unwrap_or_else(|_| Err("web ask 任务已结束".to_string()))
        };

        Ok(WebStreamHandle {
            chunks: Box::pin(out_rx),
            final_reply: Box::pin(final_reply),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 非登录类错误（无前缀）原样透传
    #[test]
    fn worker_error_without_prefix_passthrough() {
        let msg = "等待回复超时（网络层与 DOM 均无有效内容）".to_string();
        match classify_worker_error(msg.clone()) {
            WorkerAskError::Other(out) => assert_eq!(out, msg),
            WorkerAskError::LoginRequired(_) => panic!("普通错误不应被识别为登录失效"),
        }
    }

    /// 登录类错误剥离内部前缀，保留可读指引
    #[test]
    fn auth_required_prefix_stripped() {
        let msg = format!("{ERR_NOT_LOGGED_IN}尚未登录：请重新授权");
        match classify_worker_error(msg) {
            WorkerAskError::LoginRequired(base) => {
                assert_eq!(base, "尚未登录：请重新授权");
                assert!(!base.contains(ERR_NOT_LOGGED_IN));
            }
            WorkerAskError::Other(_) => panic!("登录错误应被单独识别"),
        }
    }

    /// 重试后仍失败时给出可操作指引，不再提示「窗口已拉起、稍后重试」
    #[test]
    fn login_still_invalid_is_actionable() {
        let out = login_still_invalid("deepseek", "尚未登录");
        assert!(out.contains("deepseek"));
        assert!(out.contains("重新授权"));
    }
}
