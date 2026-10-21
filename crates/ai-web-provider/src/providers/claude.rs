//! Claude（claude.ai）平台 provider：CDP 授权 / cookie 导入登录、
//! ask（网络层 SSE 捕获 + DOM 兜底）、cookie 持久化免登录、
//! reqwest 拦截代理。
//!
//! 站点要点（2026-10 知识，选择器写得宽松以便随页面演进容错）：
//! - 必须登录才能对话（无匿名模式）：未登录访问首页自动跳转
//!   claude.ai/login（URL 判定登录态的主要依据）；登录成功后回到
//!   /new 或 /chats/{id} 并渲染聊天输入框。
//! - 无扫码 / 无短信登录：登录走邮箱验证码 / 密码 / Google / Apple，
//!   登录页有 Cloudflare Turnstile，自动化不可靠 → LoginQr /
//!   LoginPhone / LoginVerify 返回明确错误。登录正道是 CDP 浏览器
//!   授权（POST /api/web-providers/claude/login/cdp，即前端「网页版
//!   账号」页的「授权登录」），验证通过后自动持久化
//!   （storage/claude/cookies.json，重启恢复）。
//! - 关键 cookie：`sessionKey`（HttpOnly，`sk-ant-sid01-` 前缀，
//!   登录后 .claude.ai 域）。
//! - 输入框：div.ProseMirror[contenteditable="true"]（tiptap 编辑器，
//!   build_fill_js 通用策略可覆盖）。
//! - 发送按钮：button[aria-label="Send message"]（生成中消失/变为
//!   Stop 按钮）；Enter 直接发送（Shift+Enter 换行）。
//! - 消息结构：div[data-testid="assistant-message"]（新）/
//!   div.font-claude-message（旧）为 assistant 回复，
//!   div[data-testid="user-message"] 为用户消息。
//! - claude.ai 有 Cloudflare 边缘防护：首访可能出 "Just a moment..."
//!   挑战页（同 ChatGPT 处理：检测并报清晰错误，需配代理）。
//! - 免费账号限额墙：用完额度显示 "You've reached your limit" → 报错。
//! - 聊天 API 为 POST claude.ai/api/organizations/{org}/chat_conversations/
//!   {uuid}/completion（SSE：`data: {"completion":"文本增量"}`，
//!   event: ping / message_stop 等控制事件忽略）。网络层捕获：响应
//!   完整到达的瞬间解析全部 completion 增量，按原始增量序列立即回放
//!   给流式客户端（obscura one-chunk stream 约束下的最优形态，同
//!   ChatGPT）。DOM 轮询仅作网络未捕获时的兜底。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use obscura::{Browser, InterceptResolution};
use tokio::sync::mpsc;

use crate::common::chunk::{encode_chunk, ChunkKind};
use crate::common::cookie_import::parse_cookie_import;
use crate::common::fill::build_fill_js;
use crate::common::util::{debug_html_dir, trace_page_html, truncate_utf8};
use crate::config::ProviderCfg;
use crate::models::*;
use crate::providers::Provider;

pub struct ClaudeProvider;

impl Provider for ClaudeProvider {
    const NAME: &'static str = "claude";
    const DISPLAY: &'static str = "Claude";
    const DEFAULT_URL: &'static str = "https://claude.ai/";
    const URL_ENV: &'static str = "CLAUDE_URL";

    async fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) {
        let browser = match Browser::builder()
            .stealth(true)
            .storage_dir(&cfg.storage_dir)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[claude] obscura 启动失败: {e}");
                return;
            }
        };
        let page = match browser.new_page().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[claude] 新建页面失败: {e}");
                return;
            }
        };
        Claude::new(browser, page, cfg).run(rx).await;
    }
}

/// 网络层捕获的一次完整对话回复：拼好的全文 + 原始增量序列（流式回放用）。
#[derive(Clone)]
struct CapturedReply {
    reply: AiReply,
    deltas: Vec<(ChunkKind, String)>,
}

pub struct Claude {
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
/// claude.ai 首访 / 风控升级时可能出现 "Just a moment..." 人机验证页。
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

/// 登录态：URL 不在登录/注册路由、无可见 Log in / 登录 入口、
/// 且存在聊天输入框（claude.ai 必须登录，未登录访问首页自动跳 /login，
/// 登录页只有邮箱 input 无 contenteditable 编辑器，输入框信号不误伤）。
const LOGGED_IN_JS: &str = r#"(() => {
  if (/\/(log-?in|sign-?in|signup|register)([/?#]|$)/i.test(location.href)) return false;
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const txtBtn = [...document.querySelectorAll('button, a')]
    .find(el => vis(el) && /^(log in|sign in|log into claude|登录)$/i.test((el.textContent || '').trim()));
  if (txtBtn) return false;
  const input = document.querySelector('.ProseMirror[contenteditable="true"], div[contenteditable="true"], textarea');
  return !!input;
})()"#;

/// 读取最后一条 AI 回复文本。
/// [data-testid="assistant-message"]（新）/ .font-claude-message（旧）
/// 取最后一个可见且非空的；克隆后剔除按钮/操作栏/代码块头部，避免
/// 「复制/重试」与代码块语言标签混入。
const READ_LAST_JS: &str = r#"(() => {
  const clean = (node) => {
    const clone = node.cloneNode(true);
    clone.querySelectorAll('button, [class*="action"], [class*="toolbar"], [class*="operate"], svg, [data-testid*="code-block"], [class*="header"]')
      .forEach(n => n.remove());
    return (clone.textContent || '').trim();
  };
  const msgs = [...document.querySelectorAll('[data-testid="assistant-message"], .font-claude-message')];
  for (let i = msgs.length - 1; i >= 0; i--) {
    const m = msgs[i];
    if (!(m.offsetParent !== null || m.getClientRects().length > 0)) continue;
    const t = clean(m);
    if (t) return t;
  }
  return '';
})()"#;

/// 还在生成：Stop 按钮可见且未禁用（生成完成后消失/换回发送按钮）。
const GENERATING_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const btn = [...document.querySelectorAll('button')]
    .find(b => !b.disabled && vis(b) && /stop/i.test(b.getAttribute('aria-label') || ''));
  if (btn) return true;
  return !!document.querySelector('[data-is-streaming="true"]');
})()"#;

/// assistant 消息条数（发送生效校验用）。
const ASSISTANT_COUNT_JS: &str = r#"document.querySelectorAll('[data-testid="assistant-message"], .font-claude-message').length"#;

/// 免费额度限额墙检测：返回 'limit-wall' 或 ''。
/// 免费账号额度用完后显示 "You've reached your limit"（数小时后重置）。
const LIMIT_WALL_JS: &str = r#"(() => {
  const txt = (document.body.innerText || '').slice(0, 20000);
  if (/you.{0,4}ve reached (your )?(usage |message )?limit|usage limit reached|limit resets at/i.test(txt)) {
    return 'limit-wall';
  }
  return '';
})()"#;

/// 点击发送按钮：aria-label 精确优先 → 宽松匹配 → Enter 键兜底。
/// 返回 button / enter / none。
const CLICK_SEND_JS: &str = r#"(() => {
  let btn = document.querySelector('button[aria-label="Send message"]');
  if (btn && !btn.disabled && btn.offsetWidth > 0) { btn.click(); return 'button'; }
  btn = [...document.querySelectorAll('button')].find(b =>
    !b.disabled && b.offsetWidth > 0 && /^(send message|send|发送)$/i.test(b.getAttribute('aria-label') || ''));
  if (btn) { btn.click(); return 'button'; }
  const el = document.querySelector('.ProseMirror[contenteditable="true"], [contenteditable="true"], textarea');
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
  const el = document.querySelector('.ProseMirror[contenteditable="true"], [contenteditable="true"], textarea');
  if (!el) return false;
  return (el.textContent || el.value || '').trim() === '';
})()"#;

impl Claude {
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
        let Some(path) = &self.cfg.cookie_path else {
            return;
        };
        match self.browser.cookies().save_to_file(path) {
            Ok(()) => {}
            Err(e) => tracing::warn!("[claude] 保存 cookies 失败: {e}"),
        }
    }

    /// 导入真实浏览器导出的 cookie 并校验登录态。
    /// Claude 的 Google 认证账号没有独立密码、无头引擎又过不了
    /// Turnstile，唯一可靠登录路径 = 真实浏览器登录（含 Google OAuth）
    /// 后导出 cookie 导入网关。关键 cookie 是 `sessionKey`
    /// （HttpOnly，登录后 .claude.ai 域）。
    async fn import_cookies(&mut self, raw: String) -> anyhow::Result<ImportCookiesResponse> {
        let entries = parse_cookie_import(&raw);
        if entries.is_empty() {
            anyhow::bail!(
                "未解析到任何 cookie。支持格式：\
                 1) EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组；\
                 2) 每行一条 `name=value; Domain=.claude.ai; Path=/` Set-Cookie 串"
            );
        }
        let has_session = entries.iter().any(|e| e.contains("sessionKey="));
        let mut imported = 0usize;
        for entry in &entries {
            match self.browser.cookies().set(entry, &self.cfg.base_url) {
                Ok(()) => imported += 1,
                Err(e) => tracing::warn!("[claude] cookie 导入失败（{entry}）: {e}"),
            }
        }

        // 刷新页面验证登录态（sessionKey 有效则不再跳 /login，
        // 渲染聊天输入框）。goto 超时 ≠ DOM 不可用，先 settle 再判定；
        // 判定未登录且导航曾超时时重试一次（排除"页面没加载完导致假阴性"）
        let nav_err = self.page.goto(&self.cfg.base_url).await.err();
        if let Some(e) = &nav_err {
            tracing::warn!("[claude] 导航等待 load 超时（{e}），页面可能已部分渲染，继续判定");
        }
        self.page.settle(3000).await;
        let mut logged_in = self.is_logged_in();
        if !logged_in && nav_err.is_some() {
            let _ = self.page.goto(&self.cfg.base_url).await;
            self.page.settle(3000).await;
            logged_in = self.is_logged_in();
        }
        if logged_in {
            self.save_cookies();
        }
        let message = if logged_in {
            format!("导入 {imported} 条 cookie，登录态已恢复并持久化到 storage")
        } else if !has_session {
            format!(
                "导入 {imported} 条 cookie 但未登录：缺少关键 cookie `sessionKey`\
                 （.claude.ai 域，HttpOnly）。请确认导出时已登录 claude.ai\
                 且导出的是 claude.ai 的 cookie"
            )
        } else {
            format!(
                "导入 {imported} 条 cookie 但未登录：sessionKey 可能已过期。\
                 请在浏览器重新登录 claude.ai 后重新授权"
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
                "[claude] 启动导航后疑似 Cloudflare 挑战页；若持续无响应，\
                 需为进程配置可达代理（CLAUDE_URL 指向镜像站亦可）"
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
                    // ==（而非 >=）：240 后 tick 持续增长会导致每次心跳都落盘
                    if self.cookie_tick == 240 {
                        self.save_cookies();
                    }
                }
            }
        }
        self.save_cookies();
        intercept_task.abort();
    }

    /// 注册 on_response 回调（completion SSE 捕获）。
    /// obscura 的 on_response 在响应完整到达后触发（one-chunk stream，
    /// 无逐 token 增量），捕获到即解析全部 completion 增量存入
    /// reply_capture，wait_reply 优先消费它——响应到达瞬间就把原始
    /// 增量序列回放给流式客户端，不再等 DOM 渲染稳定。同 qianwen：
    /// 回调可能停止触发，每次 do_ask 前重新注册。
    fn register_response_callback(&mut self) {
        if let Some(id) = self.response_cb_id.take() {
            self.page.off_response(id);
        }
        let reply_capture = self.reply_capture.clone();
        let id = self.page.on_response(Arc::new(
            move |req: &obscura::RequestInfo, resp: &obscura::Response| {
                let url = resp.url.as_str();
                // 聊天 API：claude.ai/api/.../chat_conversations/{uuid}/completion
                // （POST + SSE）。匹配 /completion 结尾（ends_with），避免
                // contains 误捕 /completion_xxx 类端点污染捕获
                if req.method != "POST" || !url.ends_with("/completion") {
                    return;
                }
                let body_str = String::from_utf8_lossy(&resp.body);
                // SSE 原文落盘（系统临时目录），便于核对协议
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let path = debug_html_dir().join(format!("{ts}_claude_sse.txt"));
                if let Err(e) = std::fs::write(&path, body_str.as_bytes()) {
                    tracing::warn!("[claude] SSE 响应落盘失败: {e}");
                }
                if let Some(cap) = parse_completion_sse(&body_str) {
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
        let is_claude_cdn = url_str.contains("claude.ai")
            || url_str.contains("anthropic.com")
            || url_str.contains("claudeusercontent.com")
            || url_str.contains("statsig.com")
            || url_str.contains("posthog.com")
            || url_str.contains("sentry.io")
            || url_str.contains("jsdelivr.net")
            || url_str.contains("gstatic.com")
            || url_str.contains("googletagmanager.com");
        if !is_claude_cdn {
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
                        let body_b64 = base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            &body,
                        );
                        let body_text = String::from_utf8_lossy(&body).into_owned();
                        let _ = req.resolver.send(InterceptResolution::Fulfill {
                            status,
                            headers,
                            body: body_text,
                            body_base64: body_b64,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("[claude] 代理读取body失败: {url_str} → {e}");
                        let _ = req.resolver.send(InterceptResolution::Fail {
                            reason: format!("proxy body read error: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[claude] 代理请求失败: {url_str} → {e}");
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
                    "Claude 无扫码登录。登录方式：网页版账号管理页「授权登录」\
                     （CDP 拉起本地 Chrome 完成登录，\
                     即 POST /api/web-providers/claude/login/cdp）"
                )));
            }
            Command::LoginPhone { phone: _, reply_tx } => {
                let _ = reply_tx.send(Err(anyhow::anyhow!(
                    "Claude 不支持手机号登录。请用网页版账号管理页「授权登录」\
                     （POST /api/web-providers/claude/login/cdp）"
                )));
            }
            Command::LoginVerify { code: _, reply_tx } => {
                let _ = reply_tx.send(Err(anyhow::anyhow!(
                    "Claude 不支持验证码登录。请用网页版账号管理页「授权登录」\
                     （POST /api/web-providers/claude/login/cdp）"
                )));
            }
            Command::ImportCookies {
                cookies, reply_tx, ..
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
                let path = std::env::temp_dir().join(format!("claude-screenshot-{ts}.html"));
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
        self.page.evaluate(GENERATING_JS).as_bool().unwrap_or(false)
    }

    fn assistant_count(&self) -> u64 {
        self.page.evaluate(ASSISTANT_COUNT_JS).as_u64().unwrap_or(0)
    }

    fn cf_challenge(&self) -> String {
        self.page
            .evaluate(CF_CHALLENGE_JS)
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    fn limit_wall(&self) -> bool {
        self.page.evaluate(LIMIT_WALL_JS).as_str().unwrap_or("") == "limit-wall"
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
        let path = std::env::temp_dir().join(format!("claude-{ts}.html"));
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
        trace_page_html(&self.page, self.debug_html, &format!("claude_{step}"));
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
                 可为进程配置代理后重启，或 CLAUDE_URL 换成可达镜像；\
                 GET /api/web-providers/claude/debug 可查看当前页状态"
            );
        }

        // claude.ai 必须登录才能对话（无匿名模式）
        if !self.is_logged_in() {
            anyhow::bail!(
                "{}尚未登录 claude.ai。请到网页版账号管理页「授权登录」\
                 （拉起本地 Chrome 完成 CDP 授权，\
                 即 POST /api/web-providers/claude/login/cdp）",
                crate::models::ERR_NOT_LOGGED_IN
            );
        }

        // 清除上一轮捕获，重新注册 on_response 回调
        // （同 qianwen/chatgpt：回调可能停止触发，每轮 ask 前重注册）
        *self.reply_capture.lock().unwrap() = None;
        self.register_response_callback();

        let count_before = self.assistant_count();
        self.send_message(&question, count_before).await?;
        let reply = self.wait_reply(&question, count_before, chunk_tx).await?;
        Ok(reply)
    }

    /// 填入问题并点击发送。发送后校验：输入框清空 或 新增 assistant 消息。
    /// 校验失败时先等 1.5s 复检（渲染慢），再重发一次（React 状态未同步
    /// 时首击可能无效），仍失败才报错并自动落盘页面快照供排查。
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
                "找不到输入框（ProseMirror / contenteditable）。\
                 页面可能未水合或被 Cloudflare 拦截，\
                 可 GET /api/web-providers/claude/debug 查看"
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

        // 发送结果校验：输入框已清空，或 assistant 消息数增加。
        // 未生效 → 等 1.5s 复检（渲染慢）→ 重发一次 → 仍失败报错 + 快照
        let not_effective =
            |cleared: bool, count_after: u64| !cleared && count_after <= count_before;
        let mut failed = not_effective(
            self.page
                .evaluate(INPUT_CLEARED_JS)
                .as_bool()
                .unwrap_or(false),
            self.assistant_count(),
        );
        if failed {
            tracing::warn!("[claude] 首次发送未生效，等 1.5s 复检（可能渲染慢）");
            self.page.settle(1500).await;
            failed = not_effective(
                self.page
                    .evaluate(INPUT_CLEARED_JS)
                    .as_bool()
                    .unwrap_or(false),
                self.assistant_count(),
            );
        }
        if failed {
            tracing::warn!("[claude] 复检仍未生效，重试发送一次");
            self.page.evaluate(CLICK_SEND_JS);
            self.page.settle(1500).await;
            failed = not_effective(
                self.page
                    .evaluate(INPUT_CLEARED_JS)
                    .as_bool()
                    .unwrap_or(false),
                self.assistant_count(),
            );
        }
        if failed {
            self.trace_html("ask_3_send_not_effective");
            let snapshot = self.dump_html();
            anyhow::bail!(
                "发送未生效（输入框未清空且无新回复块）。\
                 可能触发 Cloudflare 风控或限额墙。页面快照已保存: {snapshot}；\
                 可 GET /api/web-providers/claude/debug 查看页面状态，\
                 或 POST /api/web-providers/claude/reset 后重试"
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
            if self.limit_wall() && self.assistant_count() <= count_before {
                self.trace_html("wait_0_limit_wall");
                anyhow::bail!(
                    "Claude 免费额度已用完（限额墙出现，数小时后重置）。\
                     可更换账号重新授权，或等待额度重置"
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
                         可能被 Cloudflare 拦截，GET /api/web-providers/claude/debug 查看",
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

// ---- Claude SSE 解析 ----

/// 解析 completion SSE 响应体。每条有效行形如
/// `data: {"completion":"文本增量","stop_reason":null,"type":"completion"}`：
/// - 正文：`completion` 字段非空字符串增量（末尾 stop 条目为空串，跳过）
/// - 思考：`thinking` 字段增量（extended thinking，宽松提取兼容变体）
/// - 其余事件（event: ping / message_stop / error 等）与 [DONE] 忽略
///
/// 返回拼好的全文 + 原始增量序列；无有效内容时 None。
fn parse_completion_sse(body: &str) -> Option<CapturedReply> {
    let mut reply = String::new();
    let mut thinking = String::new();
    let mut deltas: Vec<(ChunkKind, String)> = Vec::new();

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
        if let Some(c) = v.get("completion").and_then(|x| x.as_str()) {
            if !c.is_empty() {
                reply.push_str(c);
                deltas.push((ChunkKind::Text, c.to_string()));
            }
        }
        if let Some(t) = v.get("thinking").and_then(|x| x.as_str()) {
            if !t.is_empty() {
                thinking.push_str(t);
                deltas.push((ChunkKind::Thinking, t.to_string()));
            }
        }
    }

    if reply.is_empty() && thinking.is_empty() {
        return None;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 多条 completion 增量拼接 + thinking 增量 + 控制行容错
    #[test]
    fn completion_deltas_with_control_lines() {
        let body = concat!(
            "event: ping\n",
            "data: {\"type\":\"ping\"}\n\n",
            "data: {\"completion\":\"你好\",\"stop_reason\":null,\"type\":\"completion\"}\n\n",
            "event: completion\n",
            "data: {\"completion\":\"，世界\",\"stop_reason\":null,\"type\":\"completion\"}\n\n",
            "data: {\"completion\":\"\",\"stop_reason\":\"end_turn\",\"type\":\"completion\"}\n\n",
            "data: [DONE]\n\n",
            "event: message_stop\n",
        );
        let cap = parse_completion_sse(body).unwrap();
        assert_eq!(cap.reply.reply, "你好，世界");
        assert!(cap.reply.thinking.is_empty());
        // 空串末条 stop 记录跳过，不产生空 delta
        let joined: String = cap
            .deltas
            .iter()
            .filter(|(k, _)| matches!(k, ChunkKind::Text))
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(joined, cap.reply.reply);
        assert_eq!(cap.deltas.len(), 2);
    }

    /// thinking 增量走 ChunkKind::Thinking，与正文分离
    #[test]
    fn thinking_deltas() {
        let body = concat!(
            "data: {\"thinking\":\"先分析\",\"type\":\"thinking\"}\n\n",
            "data: {\"thinking\":\"再推理\",\"type\":\"thinking\"}\n\n",
            "data: {\"completion\":\"结论\"}\n",
        );
        let cap = parse_completion_sse(body).unwrap();
        assert_eq!(cap.reply.thinking, "先分析再推理");
        assert_eq!(cap.reply.reply, "结论");
        assert_eq!(cap.deltas.len(), 3);
        assert!(cap.deltas[0].0 == ChunkKind::Thinking);
        assert!(cap.deltas[2].0 == ChunkKind::Text);
    }

    /// 非 JSON 行 / 非 data 行 / 全空 completion → None
    #[test]
    fn garbage_and_empty_returns_none() {
        assert!(parse_completion_sse("").is_none());
        assert!(parse_completion_sse("event: ping\n").is_none());
        assert!(parse_completion_sse("data: not-json\n").is_none());
        assert!(parse_completion_sse("data: {\"completion\":\"\"}\n").is_none());
        assert!(parse_completion_sse("data: [DONE]\n").is_none());
        // completion 非字符串（对象/数字）不 panic、不误收
        assert!(parse_completion_sse("data: {\"completion\":{\"x\":1}}\n").is_none());
    }
}
