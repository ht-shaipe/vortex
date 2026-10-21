//! 网页版平台（浏览器自动化）服务。
//!
//! 持有 ai-web-provider 的平台 worker 集合（每平台一个 obscura 引擎线程，
//! V8 !Send），实现 [`WebChatBridge`] 供代理引擎的 WebExecutor 调用；
//! 同时内置 [`CdpLoginManager`]，编排「拉起本地 Chrome → 用户登录 →
//! CDP 收集 cookie → 注入 worker → 校验持久化」的授权流程。
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
use parking_lot::{Mutex, RwLock};
use tokio::sync::{mpsc, oneshot};
use vortex_router::proxy::web_bridge::{
    WebChatBridge, WebChunk, WebChunkKind, WebReply, WebStreamHandle,
};

/// 自动重授权的冷却窗口：与 CDP 授权等待上限对齐，窗口内不重复拉起
/// （进行中的会话由 CdpLoginManager::start 自身幂等拒绝，冷却防的是
/// 高频 ask 失败反复走到 spawn 的抖动）
const AUTO_REAUTH_COOLDOWN: Duration = Duration::from_secs(120);

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
    /// 平台 → 上次自动重授权触发时间（冷却去重）
    auto_reauth_last: Arc<Mutex<HashMap<String, Instant>>>,
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
            auto_reauth_last: Arc::new(Mutex::new(HashMap::new())),
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

    /// 收集自动重授权触发所需的上下文（'static，可 move 进异步任务）。
    /// worker 未启动等取不到通道时返回 None（此时退化为普通错误文案）
    fn reauth_ctx(&self, platform: &str) -> Option<ReauthCtx> {
        let base_url = self.base_urls.get(platform)?.clone();
        let worker_tx = self.worker_tx(platform).ok()?;
        Some(ReauthCtx {
            platform: platform.to_string(),
            base_url,
            worker_tx,
            cdp: self.cdp.clone(),
            last: self.auto_reauth_last.clone(),
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

/// ask 失败自动重授权的触发上下文
struct ReauthCtx {
    platform: String,
    base_url: String,
    worker_tx: mpsc::Sender<Command>,
    cdp: Arc<CdpLoginManager>,
    last: Arc<Mutex<HashMap<String, Instant>>>,
}

/// worker 错误 → 返回给调用方的错误文案。
/// 带未登录前缀（`ERR_NOT_LOGGED_IN`）时：自动拉起 CDP 授权 Chrome，
/// 用户在浏览器完成登录后凭证自动注入 worker，之后的请求即可用；
/// 冷却窗口内不重复拉起（进行中的会话本来也会被 CdpLoginManager 拒绝）。
/// 前缀剥离后再返回，不暴露给最终客户端。
fn map_worker_error(ctx: Option<ReauthCtx>, msg: String) -> String {
    // 前缀是内部协议标记：无论是否有重授权上下文都先剥离，
    // 不暴露给最终客户端
    let Some(base) = msg.strip_prefix(ERR_NOT_LOGGED_IN) else {
        return msg;
    };
    let base = base.trim();
    let Some(ctx) = ctx else {
        return base.to_string();
    };
    let cooldown_active = {
        let mut last = ctx.last.lock();
        match last.get(&ctx.platform) {
            Some(t) if t.elapsed() < AUTO_REAUTH_COOLDOWN => true,
            _ => {
                last.insert(ctx.platform.clone(), Instant::now());
                false
            }
        }
    };
    if cooldown_active {
        return format!(
            "{} 登录态已失效。Chrome 授权窗口此前已拉起（等待登录中），\
             请在浏览器完成登录后重试提问。{}",
            ctx.platform, base
        );
    }
    let platform_name = ctx.platform.clone();
    tokio::spawn(async move {
        if let Err(e) = ctx
            .cdp
            .start(&ctx.platform, &ctx.base_url, ctx.worker_tx)
            .await
        {
            log::warn!("[web] {} 自动重授权拉起失败: {}", ctx.platform, e);
        }
    });
    format!(
        "{} 登录态已失效，已自动拉起 Chrome 授权窗口：\
         请在浏览器完成登录，凭证会自动同步到网关，稍后重试提问即可。{}",
        platform_name, base
    )
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
        let tx = self.worker_tx(platform)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(Command::Ask {
            question,
            chunk_tx: None,
            reply_tx,
        })
        .await
        .map_err(|_| "worker 已退出".to_string())?;
        match reply_rx.await {
            Ok(Ok(r)) => Ok(to_web_reply(r)),
            Ok(Err(e)) => Err(map_worker_error(self.reauth_ctx(platform), e.to_string())),
            Err(_) => Err("worker 已退出".to_string()),
        }
    }

    async fn ask_stream(
        &self,
        platform: &str,
        question: String,
    ) -> Result<WebStreamHandle, String> {
        let tx = self.worker_tx(platform)?;
        let (chunk_tx, mut chunk_rx) = mpsc::channel::<String>(64);
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(Command::Ask {
            question,
            chunk_tx: Some(chunk_tx),
            reply_tx,
        })
        .await
        .map_err(|_| "worker 已退出".to_string())?;

        // 错误路径的自动重授权上下文（move 进 final_reply；
        // 流式路径的错误同样应触发拉起 CDP）
        let reauth_ctx = self.reauth_ctx(platform);

        // chunk 解码任务：worker 的编码 chunk（"T:xxx"/"H:xxx"）→ WebChunk 流
        let (mut out_tx, out_rx) = futures::channel::mpsc::channel::<WebChunk>(64);
        tokio::spawn(async move {
            while let Some(raw) = chunk_rx.recv().await {
                if raw.is_empty() {
                    continue;
                }
                let chunk = match decode_chunk(&raw) {
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
                        text: raw,
                    },
                };
                if out_tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });

        let final_reply = async move {
            match reply_rx.await {
                Ok(Ok(r)) => Ok(to_web_reply(r)),
                Ok(Err(e)) => Err(map_worker_error(reauth_ctx, e.to_string())),
                Err(_) => Err("worker 已退出".to_string()),
            }
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
    #[tokio::test]
    async fn worker_error_without_prefix_passthrough() {
        let msg = "等待回复超时（网络层与 DOM 均无有效内容）".to_string();
        assert_eq!(map_worker_error(None, msg.clone()), msg);
    }

    /// 登录类错误（带前缀）在无重授权上下文时剥离前缀、保留指引文案
    /// （worker_tx 取不到的退化路径，前缀不暴露给最终客户端）
    #[tokio::test]
    async fn auth_required_prefix_stripped_without_ctx() {
        let msg = format!("{ERR_NOT_LOGGED_IN}尚未登录：请重新授权");
        let out = map_worker_error(None, msg);
        assert!(!out.contains(ERR_NOT_LOGGED_IN));
        assert!(out.contains("尚未登录"));
    }
}
