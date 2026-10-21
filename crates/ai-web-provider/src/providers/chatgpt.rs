//! ChatGPT（chatgpt.com）平台 provider：匿名对话开箱即用、cookie 持久化
//! 保持登录态、网络层 SSE 捕获（响应到达即回放增量）、DOM 轮询兜底、
//! reqwest 拦截代理。
//!
//! 站点要点（2026-10 知识，选择器写得宽松以便随页面演进容错）：
//! - 无扫码 / 无短信登录：OpenAI 不支持 +86 手机号，邮箱+密码在
//!   auth.openai.com 有 Cloudflare Turnstile，自动化不可靠；Google 认证
//!   账号无独立密码且 Google 页面对无头引擎有强风控 → LoginQr /
//!   LoginPhone / LoginVerify 返回明确错误。登录正道是
//!   POST /api/chatgpt/login/cookie：真实浏览器（Google 认证）登录
//!   chatgpt.com 后用扩展导出 cookie 导入，验证通过后自动持久化
//!   （storage/chatgpt/cookies.json，重启恢复）；也可用 /api/chatgpt/debug
//!   + /click + /eval 工具人工辅助登录。
//! - 未登录可匿名聊天（免费额度，轮数有限，超限弹登录墙 → 报错提示）。
//! - 输入框：#prompt-textarea（contenteditable ProseMirror，build_fill_js
//!   通用策略可覆盖）。
//! - 发送按钮：button[data-testid="send-button"]；生成中变为
//!   data-testid="stop-button"（作为「还在生成」信号）。
//! - 回复块：div[data-message-author-role="assistant"]，取最后一条。
//! - chatgpt.com 有 Cloudflare 边缘防护：首访可能出 "Just a moment..."
//!   挑战页，obscura 未必能过 → 启动/ask 前检测并报清晰错误（需配代理）。
//! - 聊天 API 为 POST /backend-api/conversation（SSE，树状增量协议：
//!   data: {"p":"/message/content/parts/0","o":"append","v":"文本"}）。
//!   网络层捕获：响应完整到达的瞬间解析全部 append 事件，按原始增量
//!   序列立即回放给流式客户端——比 DOM 稳定检测早得多。注意 obscura
//!   的 fetch 是 one-chunk stream（op_fetch_url 整体读齐 body 才交给
//!   JS），逐 token 转发做不到；「SSE 到达即回放」是现有引擎约束下的
//!   最优形态。DOM 轮询仅作网络未捕获时的兜底。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use obscura::{Browser, InterceptResolution};
use tokio::sync::mpsc;

use crate::common::chunk::{encode_chunk, ChunkKind};
use crate::common::cookie_import::parse_cookie_import;
use crate::common::fill::build_fill_js;
use crate::common::util::{trace_page_html, truncate_utf8};
use crate::config::ProviderCfg;
use crate::models::*;
use crate::providers::Provider;

pub struct ChatgptProvider;

impl Provider for ChatgptProvider {
    const NAME: &'static str = "chatgpt";
    const DISPLAY: &'static str = "ChatGPT";
    const DEFAULT_URL: &'static str = "https://chatgpt.com/";
    const URL_ENV: &'static str = "CHATGPT_URL";

    async fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) {
        let browser = match Browser::builder()
            .stealth(true)
            .storage_dir(&cfg.storage_dir)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[chatgpt] obscura 启动失败: {e}");
                return;
            }
        };
        let page = match browser.new_page().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[chatgpt] 新建页面失败: {e}");
                return;
            }
        };
        Chatgpt::new(browser, page, cfg).run(rx).await;
    }
}

/// 网络层捕获的一次完整对话回复：拼好的全文 + 原始增量序列（流式回放用）。
#[derive(Clone)]
struct CapturedReply {
    reply: AiReply,
    deltas: Vec<(ChunkKind, String)>,
}

pub struct Chatgpt {
    browser: Browser,
    page: obscura::Page,
    cfg: Arc<ProviderCfg>,
    debug_html: bool,
    page_recreated: bool,
    navigate_after_recreate: bool,
    cookie_tick: u32,
    reply_capture: Arc<Mutex<Option<CapturedReply>>>,
    response_cb_id: Option<u64>,
}

// ---- JS 片段 ----

/// Cloudflare 挑战页检测：返回 'cf-challenge' 或 ''。
/// chatgpt.com 首访 / 风控升级时可能出现 "Just a moment..." 人机验证页。
const CF_CHALLENGE_JS: &str = r#"(() => {
  const t = document.title || '';
  if (/just a moment|checking your browser|attention required|verify you are human/i.test(t)) {
    return 'cf-challenge';
  }
  if (document.querySelector('#challenge-form, #challenge-error-text, #challenge-running')) {
    return 'cf-challenge';
  }
  return '';
})()"#;

/// 登录态：无可见 Log in / 登录 入口则已登录。
/// ChatGPT 未登录时顶部 nav 有 [data-testid="login-button"]（"Log in"）；
/// 登录后消失（换成账号菜单）。匿名聊天期间也恒有该按钮。
/// URL 防线（最高优先级）：token 失效被 next-auth 整页跳 /auth/login
/// 或 OAuth 中转（auth.openai.com）时，中转页无 login-button 会走
/// 负向兜底误判已登录。
const LOGGED_IN_JS: &str = r#"(() => {
  if (/\/(auth\/)?(login|log-in|signin|sign-in|signup|register)([/?#]|$)/i.test(location.pathname)
      || /auth\.openai\.com/i.test(location.host)) return false;
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const login = document.querySelector('[data-testid="login-button"], [data-testid="login-button-mobile"]');
  if (login && vis(login)) return false;
  const txtBtn = [...document.querySelectorAll('button, a')]
    .find(el => vis(el) && /^(log in|登录)$/i.test((el.textContent || '').trim()));
  return !txtBtn;
})()"#;

/// 读取最后一条 AI 回复文本。
/// [data-message-author-role="assistant"] 取最后一个可见且非空的；
/// 克隆后剔除按钮/操作栏/图标，避免「复制/重试/朗读」等混入。
const READ_LAST_JS: &str = r#"(() => {
  const clean = (node) => {
    const clone = node.cloneNode(true);
    clone.querySelectorAll('button, [class*="action"], [class*="toolbar"], [class*="operate"], svg')
      .forEach(n => n.remove());
    return (clone.textContent || '').trim();
  };
  const msgs = [...document.querySelectorAll('[data-message-author-role="assistant"]')];
  for (let i = msgs.length - 1; i >= 0; i--) {
    const m = msgs[i];
    if (!(m.offsetParent !== null || m.getClientRects().length > 0)) continue;
    const t = clean(m);
    if (t) return t;
  }
  return '';
})()"#;

/// 还在生成：停止按钮可见且未禁用（生成完成后消失/换回发送按钮）。
const GENERATING_JS: &str = r#"(() => {
  const btn = document.querySelector('button[data-testid="stop-button"], button[data-testid="speech-icon-stop"], button[aria-label="Stop"], button[aria-label="停止"]');
  return !!(btn && !btn.disabled && (btn.offsetParent !== null || btn.getClientRects().length > 0));
})()"#;

/// assistant 消息条数（发送生效校验用）。
const ASSISTANT_COUNT_JS: &str =
    r#"document.querySelectorAll('[data-message-author-role="assistant"]').length"#;

/// 匿名额度登录墙检测：返回 'login-wall' 或 ''。
/// 匿名聊够轮数后 ChatGPT 会弹 "Sign up to continue chatting" 之类的墙。
const LOGIN_WALL_JS: &str = r#"(() => {
  const txt = (document.body.innerText || '').slice(0, 20000);
  if (/sign up to continue|log in to continue|you.{0,4}ve reached the (free )?limit|continue chatting/i.test(txt)) {
    return 'login-wall';
  }
  return '';
})()"#;

/// 点击发送按钮：data-testid 优先 → aria-label 宽松匹配 → Enter 键兜底。
/// 返回 button / enter / none。
const CLICK_SEND_JS: &str = r#"(() => {
  let btn = document.querySelector('button[data-testid="send-button"]');
  if (btn && !btn.disabled && btn.offsetWidth > 0) { btn.click(); return 'button'; }
  btn = [...document.querySelectorAll('button')].find(b =>
    !b.disabled && b.offsetWidth > 0 && /send|发送/i.test(b.getAttribute('aria-label') || ''));
  if (btn) { btn.click(); return 'button'; }
  const el = document.querySelector('#prompt-textarea, [contenteditable="true"], textarea');
  if (el) {
    el.focus();
    el.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keypress', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keyup', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}));
    return 'enter';
  }
  return 'none';
})()"#;

/// 输入框是否已清空（发送生效校验用）。
const INPUT_CLEARED_JS: &str = r#"(() => {
  const el = document.querySelector('#prompt-textarea, [contenteditable="true"], textarea');
  if (!el) return false;
  return (el.textContent || el.value || '').trim() === '';
})()"#;

impl Chatgpt {
    pub fn new(browser: Browser, page: obscura::Page, cfg: Arc<ProviderCfg>) -> Self {
        Self {
            browser,
            page,
            debug_html: cfg.debug_html,
            cfg,
            page_recreated: false,
            navigate_after_recreate: false,
            cookie_tick: 0,
            reply_capture: Arc::new(Mutex::new(None)),
            response_cb_id: None,
        }
    }

    fn save_cookies(&self) {
        let Some(path) = &self.cfg.cookie_path else { return };
        match self.browser.cookies().save_to_file(path) {
            Ok(()) => {}
            Err(e) => tracing::warn!("[chatgpt] 保存 cookies 失败: {e}"),
        }
    }

    /// 导入真实浏览器导出的 cookie 并校验登录态。
    /// ChatGPT 的 Google 认证账号没有独立密码、无头引擎又过不了
    /// Google 风控 + Turnstile，唯一可靠登录路径 = 真实浏览器登录
    /// （含 Google OAuth）后导出 cookie 导入网关。关键 cookie 是
    /// `__Secure-next-auth.session-token`（约 30 天，活跃使用自动续期）。
    async fn import_cookies(&mut self, raw: String) -> anyhow::Result<ImportCookiesResponse> {
        let entries = parse_cookie_import(&raw);
        if entries.is_empty() {
            anyhow::bail!(
                "未解析到任何 cookie。支持格式：\
                 1) EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组；\
                 2) 每行一条 `name=value; Domain=chatgpt.com; Path=/` Set-Cookie 串"
            );
        }
        let has_session_token = entries
            .iter()
            .any(|e| e.contains("__Secure-next-auth.session-token="));
        let mut imported = 0usize;
        for entry in &entries {
            match self.browser.cookies().set(entry, &self.cfg.base_url) {
                Ok(()) => imported += 1,
                Err(e) => tracing::warn!("[chatgpt] cookie 导入失败（{entry}）: {e}"),
            }
        }

        // 刷新页面验证登录态（session-token 有效则顶部 Log in 消失）
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        let logged_in = self.is_logged_in();
        if logged_in {
            self.save_cookies();
        }
        let message = if logged_in {
            format!("导入 {imported} 条 cookie，登录态已恢复并持久化到 storage")
        } else if !has_session_token {
            format!(
                "导入 {imported} 条 cookie 但未登录：缺少关键 cookie \
                 `__Secure-next-auth.session-token`（chatgpt.com 域，HttpOnly）。\
                 请确认导出时已登录 chatgpt.com 且导出的是 chatgpt.com 的 cookie"
            )
        } else {
            format!(
                "导入 {imported} 条 cookie 但未登录：session-token 可能已过期或被刷新\
                 （每次真实浏览器登录都会轮换）。请在浏览器重新登录 chatgpt.com 后\
                 重新导出导入"
            )
        };
        Ok(ImportCookiesResponse {
            imported,
            logged_in,
            message,
        })
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let proxy_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // 须短于 CDN 服务端 keep-alive（~10s），避免复用已被关闭的空闲连接
            .pool_idle_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");

        // 拦截代理独立任务并发运行：ask/导航期间拦截请求才能被及时 resolve
        let mut intercept_task = self.spawn_intercept_proxy(proxy_client.clone());

        // 启动即导航：否则首次 status/debug 评估的是空白页
        // （有 cookie 时恢复登录态也依赖导航到站点）
        if self.page.goto(&self.cfg.base_url).await.is_ok() {
            self.page.settle(3000).await;
        }
        if !self.cf_challenge().is_empty() {
            tracing::warn!(
                "[chatgpt] 启动导航后疑似 Cloudflare 挑战页；若持续无响应，\
                 需为进程配置可达代理（CHATGPT_URL 指向镜像站亦可）"
            );
        }

        // 心跳泵：obscura 没有后台事件循环，页面的 setTimeout/XHR 回调只在
        // settle() 期间推进。每 250ms 泵 200ms，命令最多等待 250ms。
        let mut heartbeat = tokio::time::interval(Duration::from_millis(250));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if self.page_recreated {
                self.page_recreated = false;
                intercept_task.abort();
                intercept_task = self.spawn_intercept_proxy(proxy_client.clone());
                self.register_response_callback();
                // reset 换新页后回到站点（cookie 在存储层，登录态保留）
                if self.navigate_after_recreate {
                    self.navigate_after_recreate = false;
                    if self.page.goto(&self.cfg.base_url).await.is_ok() {
                        self.page.settle(3000).await;
                    }
                }
            }
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(cmd) => self.handle_command(cmd).await,
                        None => break,
                    }
                }
                _ = heartbeat.tick() => {
                    self.page.settle(200).await;
                    self.cookie_tick = self.cookie_tick.wrapping_add(1);
                    if self.cookie_tick >= 240 {
                        self.save_cookies();
                    }
                }
            }
        }
        self.save_cookies();
        intercept_task.abort();
    }

    /// 注册 on_response 回调（/backend-api/conversation SSE 捕获）。
    /// obscura 的 on_response 在响应完整到达后触发（one-chunk stream，
    /// 无逐 token 增量），捕获到即解析全部 append 事件存入 reply_capture，
    /// wait_reply 优先消费它——响应到达瞬间就把原始增量序列回放给流式
    /// 客户端，不再等 DOM 渲染稳定。同 qianwen：回调可能停止触发，
    /// 每次 do_ask 前重新注册。
    fn register_response_callback(&mut self) {
        if let Some(id) = self.response_cb_id.take() {
            self.page.off_response(id);
        }
        let reply_capture = self.reply_capture.clone();
        let id = self.page.on_response(Arc::new(
            move |req: &obscura::RequestInfo, resp: &obscura::Response| {
                let url = resp.url.as_str();
                // 聊天 API：chatgpt.com/backend-api/conversation（POST + SSE）
                if !url.contains("/backend-api/conversation") || req.method != "POST" {
                    return;
                }
                let body_str = String::from_utf8_lossy(&resp.body);
                // SSE 原文落盘（html/ 目录，已 gitignore），便于核对协议
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let path = std::path::Path::new("html").join(format!("{ts}_chatgpt_sse.txt"));
                if let Err(e) = std::fs::write(&path, body_str.as_bytes()) {
                    tracing::warn!("[chatgpt] SSE 响应落盘失败: {e}");
                }
                if let Some(cap) = parse_conversation_sse(&body_str) {
                    *reply_capture.lock().unwrap() = Some(cap);
                }
            },
        ));
        self.response_cb_id = Some(id);
    }

    fn spawn_intercept_proxy(&mut self, client: reqwest::Client) -> tokio::task::JoinHandle<()> {
        let mut intercept_rx = self.page.enable_interception();
        self.register_response_callback();
        tokio::spawn(async move {
            while let Some(req) = intercept_rx.recv().await {
                let client = client.clone();
                tokio::spawn(async move {
                    Self::handle_intercept(req, &client).await;
                });
            }
        })
    }

    /// 拦截 JS/CSS 请求用 reqwest(native-tls) 代理转发
    /// （obscura 的 rustls 加载部分 CDN 资源可能失败，同豆包/千问处理）。
    /// Fetch/XHR 原生透传：InterceptedRequest 不携带请求体，代理转发会丢失
    /// 聊天接口放在 body 里的数据。
    async fn handle_intercept(req: obscura::InterceptedRequest, client: &reqwest::Client) {
        let rt = req.resource_type.as_str();
        let url_str = req.url.as_str();

        if rt != "Script" && rt != "Stylesheet" {
            let _ = req.resolver.send(InterceptResolution::Continue {
                url: None,
                method: None,
                headers: None,
                body: None,
            });
            return;
        }
        let is_openai_cdn = url_str.contains("chatgpt.com")
            || url_str.contains("openai.com")
            || url_str.contains("oaistatic.com")
            || url_str.contains("oaiusercontent.com")
            || url_str.contains("intercom")
            || url_str.contains("sentry.io")
            || url_str.contains("jsdelivr.net");
        if !is_openai_cdn {
            let _ = req.resolver.send(InterceptResolution::Continue {
                url: None,
                method: None,
                headers: None,
                body: None,
            });
            return;
        }
        let method = req.method.as_str();
        match client
            .request(
                reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
                &req.url,
            )
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let mut headers = std::collections::HashMap::new();
                for (k, v) in resp.headers().iter() {
                    if let Ok(vs) = v.to_str() {
                        headers.insert(k.as_str().to_string(), vs.to_string());
                    }
                }
                match resp.bytes().await {
                    Ok(body) => {
                        let body_b64 =
                            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &body);
                        let body_text = String::from_utf8_lossy(&body).into_owned();
                        let _ = req.resolver.send(InterceptResolution::Fulfill {
                            status,
                            headers,
                            body: body_text,
                            body_base64: body_b64,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("[chatgpt] 代理读取body失败: {url_str} → {e}");
                        let _ = req.resolver.send(InterceptResolution::Fail {
                            reason: format!("proxy body read error: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[chatgpt] 代理请求失败: {url_str} → {e}");
                let _ = req.resolver.send(InterceptResolution::Fail {
                    reason: format!("proxy error: {e}"),
                });
            }
        }
    }

    async fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::Ask {
                question,
                chunk_tx,
                reply_tx,
            } => {
                let res = self.do_ask(question, chunk_tx).await;
                let _ = reply_tx.send(res);
            }
            Command::Status { reply_tx } => {
                let logged = self.is_logged_in();
                let _ = reply_tx.send(StatusResponse {
                    logged_in: logged,
                    qr_state: "none".to_string(),
                });
            }
            Command::LoginQr { reply_tx } => {
                let _ = reply_tx.send(Err(anyhow::anyhow!(
                    "ChatGPT 无扫码登录。登录方式：\
                     1) POST /api/chatgpt/login/cookie 导入真实浏览器（Google 认证）\
                     登录后导出的 cookie——在 Chrome 登录 chatgpt.com，用 EditThisCookie / \
                     Cookie-Editor 扩展导出后粘贴提交；\
                     2) 用 GET /api/chatgpt/debug + POST /api/chatgpt/click + /eval 工具人工辅助登录\
                     （登录成功后自动持久化 cookie）。未登录也可匿名提问（额度有限）"
                )));
            }
            Command::LoginPhone { phone: _, reply_tx } => {
                let _ = reply_tx.send(Err(anyhow::anyhow!(
                    "ChatGPT 不支持手机号登录（OpenAI 无 +86 短信通道）。\
                     Google 认证账号请用 POST /api/chatgpt/login/cookie 导入 cookie；\
                     参见 POST /api/chatgpt/login 的错误说明；未登录可匿名提问"
                )));
            }
            Command::LoginVerify { code: _, reply_tx } => {
                let _ = reply_tx.send(Err(anyhow::anyhow!(
                    "ChatGPT 不支持验证码登录。Google 认证账号请用 \
                     POST /api/chatgpt/login/cookie 导入 cookie"
                )));
            }
            Command::ImportCookies {
                cookies,
                reply_tx,
                ..
            } => {
                let res = self.import_cookies(cookies).await;
                let _ = reply_tx.send(res);
            }
            Command::Reset { reply_tx } => {
                match self.browser.new_page().await {
                    Ok(p) => {
                        self.page = p;
                        // 新 page 丢失网络拦截，主循环需重建并导航回站点
                        self.page_recreated = true;
                        self.navigate_after_recreate = true;
                        let _ = reply_tx.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply_tx.send(Err(anyhow::anyhow!(e.to_string())));
                    }
                }
            }
            Command::Debug { reply_tx } => {
                let _ = reply_tx.send(self.debug_info());
            }
            Command::DumpHtml { url, reply_tx } => {
                if let Some(url) = url {
                    let _ = self.page.goto(&url).await;
                    self.page.settle(3000).await;
                }
                let _ = reply_tx.send(self.dump_html());
            }
            Command::SetDebugHtml { enabled, reply_tx } => {
                self.debug_html = enabled.unwrap_or(!self.debug_html);
                let _ = reply_tx.send(DebugHtmlResponse {
                    debug_html: self.debug_html,
                });
            }
            Command::Screenshot { reply_tx } => {
                let html = self
                    .page
                    .evaluate("document.documentElement.outerHTML")
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let path = std::env::temp_dir().join(format!("chatgpt-screenshot-{ts}.html"));
                match std::fs::write(&path, html.as_bytes()) {
                    Ok(_) => {
                        let _ = reply_tx.send(Ok(path.to_string_lossy().to_string()));
                    }
                    Err(e) => {
                        let _ = reply_tx.send(Err(anyhow::anyhow!(e)));
                    }
                }
            }
            Command::ClickAt { x, y, reply_tx } => {
                let js = format!(
                    r#"(() => {{
                      const el = document.elementFromPoint({x}, {y});
                      if (el) {{ el.click(); return true; }}
                      return false;
                    }})()"#
                );
                let _ = self.page.evaluate(&js);
                self.page.settle(500).await;
                let _ = reply_tx.send(Ok(()));
            }
            Command::EvalJs { js, reply_tx } => {
                let val = self.page.evaluate(&js).to_string();
                let _ = reply_tx.send(val);
            }
        }
    }

    // ---- 基础状态 ----

    fn is_logged_in(&self) -> bool {
        self.page.evaluate(LOGGED_IN_JS).as_bool().unwrap_or(false)
    }

    fn read_last(&self) -> String {
        self.page
            .evaluate(READ_LAST_JS)
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    fn is_generating(&self) -> bool {
        self.page
            .evaluate(GENERATING_JS)
            .as_bool()
            .unwrap_or(false)
    }

    fn assistant_count(&self) -> u64 {
        self.page
            .evaluate(ASSISTANT_COUNT_JS)
            .as_u64()
            .unwrap_or(0)
    }

    fn cf_challenge(&self) -> String {
        self.page
            .evaluate(CF_CHALLENGE_JS)
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    fn login_wall(&self) -> bool {
        self.page
            .evaluate(LOGIN_WALL_JS)
            .as_str()
            .unwrap_or("")
            == "login-wall"
    }

    fn debug_info(&self) -> String {
        let url = self
            .page
            .evaluate("location.href")
            .as_str()
            .unwrap_or("")
            .to_string();
        let title = self
            .page
            .evaluate("document.title")
            .as_str()
            .unwrap_or("")
            .to_string();
        let logged = self.is_logged_in();
        let cf = self.cf_challenge();
        let last = self.read_last();
        format!(
            "{{\"url\":\"{url}\",\"title\":\"{}\",\"logged_in\":{logged},\"cf_challenge\":\"{cf}\",\"assistant_msgs\":{},\"last_reply\":\"{}\"}}",
            truncate_utf8(&title, 200).replace('"', "\\\""),
            self.assistant_count(),
            truncate_utf8(&last, 500).replace('"', "\\\"")
        )
    }

    fn dump_html(&self) -> String {
        let html = self
            .page
            .evaluate("document.documentElement.outerHTML")
            .as_str()
            .unwrap_or("")
            .to_string();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = std::env::temp_dir().join(format!("chatgpt-{ts}.html"));
        match std::fs::File::create(&path) {
            Ok(mut f) => {
                use std::io::Write;
                let _ = f.write_all(html.as_bytes());
                path.to_string_lossy().to_string()
            }
            Err(_) => String::new(),
        }
    }

    fn trace_html(&self, step: &str) {
        trace_page_html(&self.page, self.debug_html, &format!("chatgpt_{step}"));
    }

    // ---- 提问 ----

    async fn do_ask(
        &mut self,
        question: String,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        // Cloudflare 挑战页：页面没水合，填入必然失败，直接报清晰错误
        if self.cf_challenge() == "cf-challenge" {
            self.trace_html("ask_0_cf_challenge");
            anyhow::bail!(
                "页面处于 Cloudflare 人机验证中（obscura 未必能自动通过）。\
                 可为进程配置代理后重启，或 CHATGPT_URL 换成可达镜像；\
                 GET /api/chatgpt/debug 可查看当前页状态"
            );
        }

        // 清除上一轮捕获，重新注册 on_response 回调
        // （同 qianwen：回调可能停止触发，每轮 ask 前重注册）
        *self.reply_capture.lock().unwrap() = None;
        self.register_response_callback();

        let count_before = self.assistant_count();
        self.send_message(&question, count_before).await?;
        let reply = self.wait_reply(&question, count_before, chunk_tx).await?;
        Ok(reply)
    }

    /// 填入问题并点击发送。发送后校验：输入框清空 或 新增 assistant 消息。
    async fn send_message(&mut self, text: &str, count_before: u64) -> anyhow::Result<()> {
        let fill_js = build_fill_js(text)?;
        let fill_result = self
            .page
            .evaluate(&fill_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        if fill_result == "no-input" {
            anyhow::bail!(
                "找不到输入框（#prompt-textarea / contenteditable）。\
                 页面可能未水合或被 Cloudflare 拦截，可 GET /api/chatgpt/debug 查看"
            );
        }
        if !fill_result.starts_with("ok-") {
            anyhow::bail!("问题文本未写入输入框（{fill_result}）");
        }
        self.page.settle(1000).await;
        self.trace_html("ask_1_after_fill");

        let sent = self
            .page
            .evaluate(CLICK_SEND_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if sent == "none" {
            anyhow::bail!("发送失败：既无发送按钮也无输入框");
        }
        self.page.settle(1000).await;
        self.trace_html("ask_2_after_send");

        // 发送后校验：输入框已清空，或 assistant 消息数增加
        let input_cleared = self
            .page
            .evaluate(INPUT_CLEARED_JS)
            .as_bool()
            .unwrap_or(false);
        let count_after = self.assistant_count();
        if !input_cleared && count_after <= count_before {
            self.trace_html("ask_3_send_not_effective");
            anyhow::bail!(
                "发送未生效（输入框未清空且无新回复块）。\
                 可能触发 Cloudflare 风控或登录墙，可 GET /api/chatgpt/debug 查看"
            );
        }
        Ok(())
    }

    /// 等回复说完；流式模式通过 chunk_tx 下发增量。
    /// 优先网络层捕获：SSE 完整到达的瞬间按原始增量序列回放（远早于
    /// DOM 渲染稳定）。网络未捕获时降级 DOM 稳定性轮询兜底。
    async fn wait_reply(
        &mut self,
        question: &str,
        count_before: u64,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        let start = Instant::now();
        // 过滤问题回显：读到与问题相同的文本不算回复
        let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let q_norm = norm(question);

        // 阶段1：宽限 10s，等网络层捕获（或 DOM 出现新回复块转兜底）
        while start.elapsed() < Duration::from_secs(10) {
            let captured = self.reply_capture.lock().unwrap().clone();
            if let Some(cap) = captured {
                return self.finish_with_capture(cap, chunk_tx).await;
            }
            if self.cf_challenge() == "cf-challenge" {
                anyhow::bail!("等待回复时页面进入 Cloudflare 验证，对话中断");
            }
            if self.login_wall() && self.assistant_count() <= count_before {
                self.trace_html("wait_0_login_wall");
                anyhow::bail!(
                    "{}匿名对话额度已用完（登录墙出现）。\
                     需要登录：请到「网页版账号」页对 ChatGPT 重新授权（CDP 浏览器登录）",
                    crate::models::ERR_NOT_LOGGED_IN
                );
            }
            if self.assistant_count() > count_before {
                // 新回复块已渲染（SSE 应已到达但未被捕获，如格式未覆盖）→ 转兜底
                break;
            }
            self.page.settle(500).await;
        }

        // 阶段2：DOM 稳定性轮询兜底（捕获晚到时优先消费且不重复流式）
        let dom_reply = |s: &str| -> String {
            let t = s.trim();
            if t.is_empty() || norm(t) == q_norm {
                String::new()
            } else {
                t.to_string()
            }
        };

        let mut last = dom_reply(&self.read_last());
        // DOM 增量是否已流出：捕获晚到时不再回放 deltas，避免内容重复
        let mut streamed_any = !last.is_empty();
        if !last.is_empty() {
            if let Some(tx) = &chunk_tx {
                let _ = tx.send(encode_chunk(ChunkKind::Text, &last)).await;
            }
        }
        self.trace_html("wait_1_first_reply");

        let mut stable_since = Instant::now();
        let deadline = start + Duration::from_millis(self.cfg.max_wait_ms);
        loop {
            let captured = self.reply_capture.lock().unwrap().clone();
            if let Some(cap) = captured {
                let ctx = if streamed_any { None } else { chunk_tx.clone() };
                return self.finish_with_capture(cap, ctx).await;
            }
            self.page.settle(1000).await;
            let cur = dom_reply(&self.read_last());
            if cur != last {
                if let Some(tx) = &chunk_tx {
                    let delta = if last.is_empty() || cur.starts_with(&last) {
                        cur[last.len()..].to_string()
                    } else {
                        cur.clone()
                    };
                    let _ = tx.send(encode_chunk(ChunkKind::Text, &delta)).await;
                    streamed_any = true;
                }
                last = cur;
                stable_since = Instant::now();
                continue;
            }
            let generating = self.is_generating();
            if !generating
                && !last.is_empty()
                && stable_since.elapsed().as_millis() as u64 >= self.cfg.stable_ms
            {
                break;
            }
            if Instant::now() > deadline {
                if last.is_empty() {
                    anyhow::bail!(
                        "{}s 内无回复（网络层与 DOM 均无内容），\
                         可能被 Cloudflare 拦截，GET /api/chatgpt/debug 查看",
                        self.cfg.max_wait_ms / 1000
                    );
                }
                break;
            }
        }
        self.trace_html("wait_2_reply_done");
        Ok(AiReply {
            thinking: String::new(),
            reply: last,
            ..Default::default()
        })
    }

    /// 网络层捕获完成：按原始增量序列回放给流式客户端，返回最终回复。
    /// chunk_tx=None 时（DOM 兜底已流过部分内容）只返回结果不回放，避免重复。
    async fn finish_with_capture(
        &mut self,
        captured: CapturedReply,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        if let Some(tx) = &chunk_tx {
            for (kind, text) in &captured.deltas {
                if !text.is_empty() {
                    let _ = tx.send(encode_chunk(*kind, text)).await;
                }
            }
        }
        self.trace_html("wait_3_net_reply");
        Ok(captured.reply)
    }
}

// ---- ChatGPT SSE 解析 ----

/// 解析 /backend-api/conversation 的 SSE 响应体（树状增量协议）。
/// 每行形如 `data: {"p":"/message/content/parts/0","o":"append","v":"文本"}`：
/// - `p` 为 JSON 指针式路径，`o` 为操作（append/patch/delete 等）
/// - 正文：`/message/content/parts/{n}` 的 append（v 为字符串增量）
/// - 思考：路径含 `reasoning` 的 append（宽松匹配，兼容 reasoning 等变体）
/// - patch 整体替换 parts/0（罕见：错误消息/重新生成）时以替换值为准，
///   并重算增量序列保证回放语义正确
/// - [DONE] / 非字符串 v / 其余路径（title、metadata、citation 等）忽略
/// 返回拼好的全文 + 原始增量序列；无有效内容时 None。
fn parse_conversation_sse(body: &str) -> Option<CapturedReply> {
    let mut reply = String::new();
    let mut thinking = String::new();
    let mut deltas: Vec<(ChunkKind, String)> = Vec::new();
    let mut text_appended = false;
    let mut reply_replaced = false;

    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        let p = v.get("p").and_then(|x| x.as_str()).unwrap_or("");
        let o = v.get("o").and_then(|x| x.as_str()).unwrap_or("");
        let text = v.get("v").and_then(|x| x.as_str()).unwrap_or("");
        let is_reasoning = p.contains("reasoning");
        let is_content = p.starts_with("/message/content/parts/");

        match o {
            "append" if !text.is_empty() => {
                if is_reasoning {
                    thinking.push_str(text);
                    deltas.push((ChunkKind::Thinking, text.to_string()));
                } else if is_content {
                    reply.push_str(text);
                    deltas.push((ChunkKind::Text, text.to_string()));
                    text_appended = true;
                }
            }
            "patch" if is_content && !text.is_empty() && text != reply => {
                reply = text.to_string();
                reply_replaced = true;
            }
            _ => {}
        }
    }

    if reply.is_empty() && thinking.is_empty() {
        return None;
    }
    // 正文被整体替换：丢弃此前正文增量（思考增量保留），改为单块全文回放
    if reply_replaced && text_appended {
        deltas.retain(|(k, _)| matches!(k, ChunkKind::Thinking));
        if !reply.is_empty() {
            deltas.push((ChunkKind::Text, reply.clone()));
        }
    }
    Some(CapturedReply {
        reply: AiReply {
            thinking,
            reply,
            ..Default::default()
        },
        deltas,
    })
}

