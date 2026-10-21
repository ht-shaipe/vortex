//! 通义千问（qianwen.com）平台 provider：短信登录、ask（思考/答案分离）、
//! cookie 持久化免登录、网络层 AI 回复捕获（SSE ground truth）。
//!
//! 站点逆向结论（2026-10，agent-browser 实测）：
//! - 未登录也渲染完整聊天 UI；右上角 `button` 文本恰为「登录」
//! - 点登录 → modal 内跨域 iframe `passport.qianwen.com/havanaone/login/login.htm`
//!   默认短信登录表单（tq=SD）；登录成功 topRedirect 跳回 returnUrl
//! - 淘宝/支付宝扫码为 chat→passport→taobao 三层嵌套跨域 iframe，无法自动化 →
//!   LoginQr 返回明确错误，引导使用 POST /login/phone + /login/verify
//! - 短信表单（passport iframe 内）：#fm-sms-login-id / #fm-smscode /
//!   .send-btn-link（获取验证码）/ button.fm-submit（登录）/
//!   #fm-agreement-checkbox（协议，默认未勾选）/ #fm-login-checkcode（图片验证码，风控时出现）
//! - 聊天输入框：DIV contenteditable（非 ProseMirror），data-placeholder="向千问提问"
//! - 发送按钮：button[aria-label="发送消息"]；生成中变为 aria-label="停止回答"
//! - 消息结构：.message-list-content-container > .chat-round（每轮），
//!   问题 .message-card-wrap.question，答案容器 [class*="wrapper-answer"]
//! - 匿名提问触发阿里 TMD 风控（chat2.qianwen.com/_____tmd_____/punish）→ 必须登录
//! - 聊天 API 在 chat2.qianwen.com（SSE 具体格式登录后 dump 到 html/ 再核对）

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use obscura::{Browser, InterceptResolution};
use tokio::sync::mpsc;

use crate::cdp_auth::discover_chrome;
use crate::common::cdp_chat::CdpChatSession;
use crate::common::chunk::{encode_chunk, ChunkKind};
use crate::common::cookie_import::parse_cookie_import;
use crate::common::fill::build_fill_js;
use crate::common::util::{trace_page_html, truncate_utf8};
use crate::config::ProviderCfg;
use crate::models::*;
use crate::providers::Provider;

pub struct QianwenProvider;

impl Provider for QianwenProvider {
    const NAME: &'static str = "qianwen";
    const DISPLAY: &'static str = "通义千问";
    const DEFAULT_URL: &'static str = "https://www.qianwen.com/chat";
    const URL_ENV: &'static str = "QIANWEN_URL";

    async fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) {
        let browser = match Browser::builder()
            .stealth(true)
            .storage_dir(&cfg.storage_dir)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[qianwen] obscura 启动失败: {e}");
                return;
            }
        };
        let page = match browser.new_page().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[qianwen] 新建页面失败: {e}");
                return;
            }
        };
        Qianwen::new(browser, page, cfg).run(rx).await;
    }
}

/// 聊天请求（/chat/completions）网络生命周期：
/// None → InFlight（请求已发出）→ Done（响应已完整到达）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionState {
    None,
    InFlight,
    Done,
}

pub struct Qianwen {
    browser: Browser,
    page: obscura::Page,
    cfg: Arc<ProviderCfg>,
    debug_html: bool,
    page_recreated: bool,
    navigate_after_recreate: bool,
    cookie_tick: u32,
    reply_capture: Arc<Mutex<Option<(AiReply, Instant)>>>,
    completion_state: Arc<Mutex<CompletionState>>,
    response_cb_id: Option<u64>,
    /// CDP Chrome 提问会话：baxia 风控对 obscura 无头环境强制滑块（真 Chrome
    /// 不触发），千问提问走 CDP Chrome；None = 未启动
    cdp_chat: Option<CdpChatSession>,
}

// ---- JS 片段（主文档） ----

/// 登录态：无可见「登录」按钮则已登录。
/// 千问未登录时右上角与侧边栏都有文本恰为「登录」的 button；
/// 登录后消失（头像菜单里是「退出登录」，精确匹配不误伤）。
/// URL 防线（最高优先级）：token 失效被 topRedirect 整页跳 passport
/// 登录页时，提交按钮文案常为「登 录/立即登录」，严格 === '登录'
/// 不命中会误判已登录。
const LOGGED_IN_JS: &str = r#"(() => {
  if (/passport|login/i.test(location.host) || /\/(login|signin|passport)([/?#]|$)/i.test(location.pathname)) return false;
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const btn = [...document.querySelectorAll('button, a')]
    .find(el => vis(el) && /^(登\s*录|立即登录|log\s*in)$/i.test((el.textContent || '').trim()));
  return !btn;
})()"#;

/// 点击「登录」按钮（打开 passport 登录弹窗）。返回 'CLICKED:描述' / 'NONE'。
const CLICK_LOGIN_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const els = [...document.querySelectorAll('button, a')]
    .filter(el => vis(el) && (el.textContent || '').trim() === '登录');
  if (!els.length) return 'NONE';
  const target = els[els.length - 1];
  for (const t of ['pointerdown', 'mousedown', 'pointerup', 'mouseup']) {
    try {
      target.dispatchEvent(new MouseEvent(t, {bubbles: true, cancelable: true, view: window}));
    } catch (e) {}
  }
  target.click();
  return 'CLICKED:' + target.tagName + '.' + String(target.className || '').slice(0, 50);
})()"#;

/// 读取最后一条 AI 回复文本。
/// 优先最后一个 .chat-round 内（及其后兄弟）的答案容器 [class*="wrapper-answer"]；
/// 克隆后剔除按钮/操作栏，避免「复制/重试」等操作文本混入。
/// 选择器随千问前端改版可能过时（.chat-round 已失效过），故末尾保留多层
/// 宽松兜底；精确选择器待对话页快照核对后收紧。
const READ_LAST_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const clean = (node) => {
    const clone = node.cloneNode(true);
    clone.querySelectorAll('button, [class*="action"], [class*="toolbar"], [class*="operate"], [class*="feedback"]')
      .forEach(n => n.remove());
    return (clone.textContent || '').trim();
  };
  const rounds = [...document.querySelectorAll('.chat-round')];
  if (rounds.length) {
    const last = rounds[rounds.length - 1];
    const scopes = [last];
    let sib = last.nextElementSibling;
    for (let i = 0; i < 3 && sib; i++) { scopes.push(sib); sib = sib.nextElementSibling; }
    for (const scope of scopes) {
      const ans = scope.querySelector('[class*="wrapper-answer"]');
      if (ans && vis(ans)) {
        const t = clean(ans);
        if (t) return t;
      }
    }
  }
  const wrappers = [...document.querySelectorAll('[class*="wrapper-answer"]')].filter(vis);
  if (wrappers.length) {
    const t = clean(wrappers[wrappers.length - 1]);
    if (t) return t;
  }
  const mds = [...document.querySelectorAll('[class*="markdown"]')].filter(vis);
  if (mds.length) return (mds[mds.length - 1].textContent || '').trim();
  const answers = [...document.querySelectorAll('[class*="answer"], [class*="Answer"], [class*="bubble"], [class*="Bubble"]')]
    .filter(vis);
  if (answers.length) {
    const t = clean(answers[answers.length - 1]);
    if (t) return t;
  }
  return '';
})()"#;

/// 是否还在生成：「停止回答」按钮可见且未禁用。
const STOPPING_JS: &str = r#"(() => {
  const btn = document.querySelector('button[aria-label="停止回答"], button[aria-label="停止生成"]');
  return !!(btn && !btn.disabled && (btn.offsetParent !== null || btn.getClientRects().length > 0));
})()"#;

/// 千问聊天 API 匹配（请求/响应 URL）：
/// 现为 chat2.qianwen.com/api/v2/chat?...（Fetch，带查询串）；历史版本为
/// /chat/completions。风控页 /api/v2/chat/_____tmd_____/punish 前缀相同但
/// "/" 紧随，用 "/api/v2/chat?" 区分
fn is_chat_api_url(url: &str) -> bool {
    url.contains("/api/v2/chat?") || url.contains("/chat/completions")
}

/// 阿里 baxia 风控滑块弹窗是否可见（.baxia-dialog 且未被关闭）
const CAPTCHA_PRESENT_JS: &str = r#"(() => {
  const d = document.querySelector('.baxia-dialog');
  return !!d && d.style.display !== 'none';
})()"#;

/// 点击发送按钮（优先 aria-label「发送消息」；退化 Enter）。返回 button/enter/none
const SEND_CLICK_JS: &str = r#"(() => {
  let btn = document.querySelector('button[aria-label="发送消息"]');
  if (btn && !btn.disabled) { btn.click(); return 'button'; }
  btn = [...document.querySelectorAll('button')].find(b =>
    !b.disabled && /发送|send/i.test(b.getAttribute('aria-label') || '') && b.offsetWidth > 0);
  if (btn) { btn.click(); return 'button'; }
  const el = document.querySelector('[contenteditable="true"], textarea');
  if (el) {
    el.focus();
    el.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keypress', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keyup', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}));
    return 'enter';
  }
  return 'none';
})()"#;

/// Slate 填入后校验 JS：文本已写入且发送按钮已启用（Slate 状态同步后
/// 按钮才从 disabled 解锁）。返回 ok / button-disabled:* / no-text:*
fn slate_check_js(text: &str) -> anyhow::Result<String> {
    Ok(format!(
        r#"((t) => {{
  const el = document.querySelector('[contenteditable="true"], textarea');
  const cur = el ? ((el.textContent || el.value || '').trim()) : '';
  const hasText = cur.includes(t);
  const btn = document.querySelector('button[aria-label="发送消息"]');
  if (hasText && btn && !btn.disabled) return 'ok';
  if (hasText) return 'button-disabled:cur=' + cur.slice(0, 30);
  return 'no-text:cur=' + cur.slice(0, 30) + ',btn=' + (btn ? String(btn.disabled) : 'missing');
}})({})"#,
        serde_json::to_string(text)?
    ))
}

// ---- JS 片段（passport 登录 iframe 内执行） ----

/// 填入手机号（#fm-sms-login-id，native setter + input/change 事件）
const FRAME_FILL_PHONE_JS: &str = r#"((phone) => {
  const el = document.querySelector('#fm-sms-login-id');
  if (!el) return 'no-input';
  el.focus();
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, phone);
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return el.value === phone ? 'ok' : 'fail';
})"#;

/// 勾选用户协议（#fm-agreement-checkbox）
const FRAME_CHECK_AGREEMENT_JS: &str = r#"(() => {
  const cb = document.querySelector('#fm-agreement-checkbox');
  if (!cb) return 'no-checkbox';
  if (!cb.checked) cb.click();
  return cb.checked ? 'checked' : 'unchecked';
})()"#;

/// 点击「获取验证码」（.send-btn-link）。返回 clicked / disabled / no-btn。
const FRAME_SEND_CODE_JS: &str = r#"(() => {
  const btn = document.querySelector('.send-btn-link');
  if (!btn) return 'no-btn';
  if (btn.classList.contains('send-btn-disabled')) return 'disabled';
  btn.click();
  return 'clicked';
})()"#;

/// 图片验证码是否出现（风控触发时 #fm-login-checkcode 变可见）
const FRAME_CAPTCHA_JS: &str = r#"(() => {
  const el = document.querySelector('#fm-login-checkcode');
  return !!(el && el.offsetWidth > 0);
})()"#;

/// 填入短信验证码（#fm-smscode）
const FRAME_FILL_CODE_JS: &str = r#"((code) => {
  const el = document.querySelector('#fm-smscode');
  if (!el) return 'no-input';
  el.focus();
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, code);
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return el.value === code ? 'ok' : 'fail';
})"#;

/// 点击「登录」提交按钮（button.fm-submit）
const FRAME_SUBMIT_JS: &str = r#"(() => {
  const btn = document.querySelector('button.fm-submit');
  if (!btn) return 'no-btn';
  btn.click();
  return 'clicked';
})()"#;

impl Qianwen {
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
            completion_state: Arc::new(Mutex::new(CompletionState::None)),
            response_cb_id: None,
            cdp_chat: None,
        }
    }

    fn save_cookies(&self) {
        let Some(path) = &self.cfg.cookie_path else { return };
        match self.browser.cookies().save_to_file(path) {
            Ok(()) => {}
            Err(e) => tracing::warn!("[qianwen] 保存 cookies 失败: {e}"),
        }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let proxy_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // 须短于 CDN 服务端 keep-alive（~10s），避免复用已被关闭的空闲连接
            .pool_idle_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");

        // 拦截代理独立任务并发运行：登录/ask 全流程期间拦截请求才能被及时 resolve
        let mut intercept_task = self.spawn_intercept_proxy(proxy_client.clone());

        // 启动即导航：否则首次 status/debug 评估的是空白页（有 cookie 时会误报未登录）
        if self.page.goto(&self.cfg.base_url).await.is_ok() {
            self.page.settle(3000).await;
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
                // reset 换新页后回到站点恢复登录态（cookie 在存储层）
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

    fn spawn_intercept_proxy(&mut self, client: reqwest::Client) -> tokio::task::JoinHandle<()> {
        let mut intercept_rx = self.setup_page_network();
        let completion_state = self.completion_state.clone();
        tokio::spawn(async move {
            while let Some(req) = intercept_rx.recv().await {
                let client = client.clone();
                let completion_state = completion_state.clone();
                tokio::spawn(async move {
                    Self::handle_intercept(req, &client, &completion_state).await;
                });
            }
        })
    }

    fn setup_page_network(
        &mut self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<obscura::InterceptedRequest> {
        let intercept_rx = self.page.enable_interception();
        self.register_response_callback();
        intercept_rx
    }

    /// 注册 on_response 回调（AI 回复 SSE 捕获）。
    /// obscura 的 on_response 在 SSE 响应后可能停止触发，需在每次 do_ask 前重新注册。
    fn register_response_callback(&mut self) {
        if let Some(id) = self.response_cb_id.take() {
            self.page.off_response(id);
        }
        let reply_capture = self.reply_capture.clone();
        let completion_state = self.completion_state.clone();
        let id = self.page.on_response(Arc::new(
            move |_req: &obscura::RequestInfo, resp: &obscura::Response| {
                let url = resp.url.as_str();
                let ct = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                let is_stream = ct.contains("event-stream");
                // 千问聊天 API：chat2.qianwen.com/api/v2/chat?...
                let is_chat_api = is_chat_api_url(url);

                if is_chat_api {
                    *completion_state.lock().unwrap() = CompletionState::Done;
                }
                if !is_stream && !is_chat_api {
                    return;
                }

                let body_str = String::from_utf8_lossy(&resp.body);
                if is_chat_api || is_stream {
                    tracing::info!(
                        "[qianwen] on_response 命中: {}（ct={}, {} 字节）",
                        url,
                        ct,
                        resp.body.len()
                    );
                }
                // SSE 聊天响应原文落盘（系统临时目录，路径不依赖进程 CWD），
                // 便于核对千问协议
                if is_stream {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let path = std::env::temp_dir().join(format!("{ts}_qianwen_sse.txt"));
                    if let Err(e) = std::fs::write(&path, body_str.as_bytes()) {
                        tracing::warn!("[qianwen] SSE 响应落盘失败: {e}");
                    } else {
                        tracing::info!("[qianwen] SSE 响应已落盘: {}", path.display());
                    }
                }
                if let Some(mut r) = extract_sse_reply(&body_str) {
                    r.thinking = r.thinking.trim().to_string();
                    r.reply = r.reply.trim().to_string();
                    if !r.reply.is_empty() || !r.thinking.is_empty() {
                        *reply_capture.lock().unwrap() = Some((r, Instant::now()));
                    }
                }
            },
        ));
        self.response_cb_id = Some(id);
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
                    "千问的淘宝/支付宝扫码登录是三层嵌套跨域 iframe（chat→passport→taobao），\
                     暂不支持自动化。请使用短信登录：POST /api/qianwen/login/phone 与 \
                     POST /api/qianwen/login/verify"
                )));
            }
            Command::LoginPhone { phone, reply_tx } => {
                let res = self.login_phone(&phone).await;
                let _ = reply_tx.send(res);
            }
            Command::LoginVerify { code, reply_tx } => {
                let res = self.login_verify(&code).await;
                let _ = reply_tx.send(res);
            }
            Command::Reset { reply_tx } => {
                // 关闭 CDP 提问 Chrome（reset 后下次 ask 重新拉起，干净状态）
                if let Some(mut session) = self.cdp_chat.take() {
                    session.shutdown().await;
                }
                match self.browser.new_page().await {
                    Ok(p) => {
                        self.page = p;
                        // 新 page 丢失网络拦截/响应捕获，主循环需重建并导航回站点
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
                let path = std::env::temp_dir().join(format!("qianwen-screenshot-{ts}.html"));
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
                // "frame:" 前缀：在 passport iframe 内求值（诊断/调试用）
                let val = if let Some(frame_js) = js.strip_prefix("frame:") {
                    match self.eval_in_passport(frame_js) {
                        Some(v) => v.to_string(),
                        None => "frame-eval-failed".to_string(),
                    }
                } else {
                    self.page.evaluate(&js).to_string()
                };
                let _ = reply_tx.send(val);
            }
            Command::ImportCookies {
                cookies,
                reply_tx,
                ..
            } => {
                let resp = self.import_cookies(cookies).await;
                let _ = reply_tx.send(resp);
            }
        }
    }

    /// 拦截 JS/CSS 请求用 reqwest(native-tls) 代理
    /// （obscura 的 rustls 加载阿里系 CDN 可能失败，同豆包/deepseek 处理）。
    /// Fetch/XHR 原生透传：InterceptedRequest 不携带请求体，代理转发会丢失
    /// 聊天/登录接口放在 body 里的数据。
    async fn handle_intercept(
        req: obscura::InterceptedRequest,
        client: &reqwest::Client,
        completion_state: &Arc<Mutex<CompletionState>>,
    ) {
        let rt = req.resource_type.as_str();
        let url_str = req.url.as_str();

        // 聊天请求在途标记（wait_reply 据此区分网络路径与 DOM 回显）
        if is_chat_api_url(url_str) {
            let mut st = completion_state.lock().unwrap();
            if *st == CompletionState::None {
                *st = CompletionState::InFlight;
            }
        }

        // 聊天相关请求可见性（排障：确认 completion 请求是否出现及真实 URL）
        if rt != "Script"
            && rt != "Stylesheet"
            && (url_str.contains("completion") || url_str.contains("chat"))
        {
            tracing::info!("[qianwen] 页面请求: {rt} {url_str}");
        }

        if rt != "Script" && rt != "Stylesheet" {
            let _ = req.resolver.send(InterceptResolution::Continue {
                url: None,
                method: None,
                headers: None,
                body: None,
            });
            return;
        }
        let is_qianwen_cdn = url_str.contains("qianwen.com")
            || url_str.contains("alicdn.com")
            || url_str.contains("aliyun")
            || url_str.contains("taobao.com")
            || url_str.contains("mmstat.com");
        if !is_qianwen_cdn {
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
                        tracing::warn!("[qianwen] 代理读取body失败: {url_str} → {e}");
                        let _ = req.resolver.send(InterceptResolution::Fail {
                            reason: format!("proxy body read error: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[qianwen] 代理请求失败: {url_str} → {e}");
                let _ = req.resolver.send(InterceptResolution::Fail {
                    reason: format!("proxy error: {e}"),
                });
            }
        }
    }

    // ---- 基础状态 ----

    fn is_logged_in(&self) -> bool {
        self.page.evaluate(LOGGED_IN_JS).as_bool().unwrap_or(false)
    }

    /// 导入真实浏览器导出的 cookie 并校验登录态。
    /// CDP 授权（cdp_auth）拉起本地 Chrome 完成登录后，收集的 cookie
    /// 转成 EditThisCookie JSON 走这里注入；也兼容扩展手动导出的导入。
    /// 关键 cookie 是 `tongyi_sso_ticket_hash`（登录后 .qianwen.com 域的
    /// 用户标识，qianwen-web 前端 USER_ID_COOKIE_KEY）。
    async fn import_cookies(&mut self, raw: String) -> anyhow::Result<ImportCookiesResponse> {
        let entries = parse_cookie_import(&raw);
        if entries.is_empty() {
            anyhow::bail!(
                "未解析到任何 cookie。支持格式：\
                 1) EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组；\
                 2) 每行一条 `name=value; Domain=.qianwen.com; Path=/` Set-Cookie 串"
            );
        }
        let has_session = entries
            .iter()
            .any(|e| e.contains("tongyi_sso_ticket_hash=") || e.contains("cookie_17="));
        let mut imported = 0usize;
        for entry in &entries {
            match self.browser.cookies().set(entry, &self.cfg.base_url) {
                Ok(()) => imported += 1,
                Err(e) => tracing::warn!("[qianwen] cookie 导入失败（{entry}）: {e}"),
            }
        }

        // 刷新页面验证登录态（阿里云登录态有效则进入对话页而非登录入口）。
        // goto 等待 load 事件：页面资源多时 30s 内 load 可能不触发——
        // 超时 ≠ DOM 不可用，先 settle 再判定；判定未登录且导航曾超时
        // 时重试一次（排除"页面没加载完导致假阴性"）
        let nav_err = self.page.goto(&self.cfg.base_url).await.err();
        if let Some(e) = &nav_err {
            tracing::warn!("[qianwen] 导航等待 load 超时（{e}），页面可能已部分渲染，继续判定");
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
                "导入 {imported} 条 cookie 但未登录：缺少关键 cookie `tongyi_sso_ticket_hash`\
                 （.qianwen.com 域登录态）。请确认导出时\
                 已登录千问且导出的是 qianwen.com 的 cookie"
            )
        } else {
            format!(
                "导入 {imported} 条 cookie 但未登录：登录态可能已过期。\
                 请在浏览器重新登录通义千问后重试授权"
            )
        };
        Ok(ImportCookiesResponse {
            imported,
            logged_in,
            message,
        })
    }

    fn read_last(&self) -> String {
        self.page
            .evaluate(READ_LAST_JS)
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    fn is_stopping(&self) -> bool {
        self.page
            .evaluate(STOPPING_JS)
            .as_bool()
            .unwrap_or(false)
    }

    /// 阿里 baxia 风控弹窗（滑块验证码 iframe）是否可见。
    /// 命中说明 /api/v2/chat 被风控拦截，回复永远不会到达
    fn captcha_present(&self) -> bool {
        self.page
            .evaluate(CAPTCHA_PRESENT_JS)
            .as_bool()
            .unwrap_or(false)
    }

    /// baxia 风控拦截时的报错信息（含处置建议）
    fn captcha_error(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "千问风控滑块验证码拦截（baxia captcha）：聊天接口 /api/v2/chat 被要求人机验证，\
             无头环境无法完成滑块。建议：\
             ① 等几分钟后重试（风控可能自动解除）；\
             ② 重新走一遍 CDP 授权：在弹出的 Chrome 中先手动给千问发一条消息\
             （若出现滑块手动完成），再点「完成授权」收集含有效 x5sec 的凭证；\
             ③ 或改用千问官方 API 渠道"
        )
    }

    /// passport 登录 iframe 在页面子 frame 中的索引（无则 None）
    fn passport_frame_index(&self) -> Option<usize> {
        self.page
            .frame_urls()
            .iter()
            .position(|u| u.contains("passport.qianwen.com"))
    }

    /// 在 passport iframe 内求值 JS
    fn eval_in_passport(&mut self, js: &str) -> Option<serde_json::Value> {
        let idx = self.passport_frame_index()?;
        match self.page.evaluate_in_frame(idx, js) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("[qianwen] iframe 求值失败: {e}");
                None
            }
        }
    }

    fn debug_info(&self) -> String {
        let url = self
            .page
            .evaluate("location.href")
            .as_str()
            .unwrap_or("")
            .to_string();
        let logged = self.is_logged_in();
        let last = self.read_last();
        let frames = self.page.frame_urls();
        let passport = frames
            .iter()
            .find(|u| u.contains("passport.qianwen.com"))
            .cloned()
            .unwrap_or_default();
        // 页面可见文本尾部摘要：debug 端点直接可见页面当前渲染内容（回复是否
        // 已显示、错误提示等），无需落盘快照
        let body_tail = self
            .page
            .evaluate("document.body.innerText")
            .as_str()
            .unwrap_or("")
            .chars()
            .rev()
            .take(600)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        format!(
            "{{\"url\":\"{url}\",\"logged_in\":{logged},\"last_reply\":\"{}\",\"frame_count\":{},\"passport_frame\":\"{}\",\"body_tail\":\"{}\"}}",
            truncate_utf8(&last, 500).replace('"', "\\\""),
            frames.len(),
            truncate_utf8(&passport, 200).replace('"', "\\\""),
            truncate_utf8(&body_tail, 600).replace('"', "\\\"").replace('\n', "\\n")
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
        let path = std::env::temp_dir().join(format!("qianwen-{ts}.html"));
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
        trace_page_html(&self.page, self.debug_html, &format!("qianwen_{step}"));
    }

    /// 开启页面 console 消息收集（纯 JS 钩子，不动 obscura——
    /// `obscura::console` 日志已过滤屏蔽脚本加载噪音，改为页面内收集、
    /// 按需取回用于排障）。幂等：重复调用只清空不重复挂钩
    fn enable_console_capture(&mut self) {
        self.page.evaluate(
            r#"(() => {
  window.__qwErrors = [];
  if (window.__qwConsoleHooked) return 'already';
  window.__qwConsoleHooked = true;
  const fmt = (args) => Array.from(args).map(a => {
    try { return (typeof a === 'object' && a !== null) ? JSON.stringify(a) : String(a); }
    catch (e) { return '[unserializable]'; }
  }).join(' ');
  const push = (prefix, args) => {
    try {
      window.__qwErrors.push((prefix ? prefix + ' ' : '') + fmt(args).slice(0, 500));
      if (window.__qwErrors.length > 100) window.__qwErrors.shift();
    } catch (e) {}
  };
  const origErr = console.error.bind(console);
  console.error = function(...args) { push('', args); origErr(...args); };
  if (console.warn) {
    const origWarn = console.warn.bind(console);
    console.warn = function(...args) { push('[warn]', args); origWarn(...args); };
  }
  window.addEventListener('error', e => {
    try { push('[uncaught]', [(e && e.message) || '']); } catch (err) {}
  });
  return 'hooked';
})()"#,
        );
    }

    /// 取回最近 n 条页面 console 输出（错误排查用），格式化为可拼进错误
    /// 信息的形式；无消息时返回空串。取回即清空
    fn console_tail(&mut self, n: usize) -> String {
        let msgs: Vec<String> = self
            .page
            .evaluate(
                r#"(() => {
  const errs = window.__qwErrors || [];
  window.__qwErrors = [];
  return JSON.stringify(errs);
})()"#,
            )
            .as_str()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();
        if msgs.is_empty() {
            return String::new();
        }
        let tail: Vec<String> = msgs
            .iter()
            .rev()
            .take(n)
            .rev()
            .map(|m| truncate_utf8(m, 300).replace('\n', " "))
            .collect();
        format!("；页面 console 最近输出: {}", tail.join(" | "))
    }

    // ---- 登录 ----

    /// 短信登录第一步：导航 → 点登录 → passport 弹窗 → 填手机号 → 勾协议 → 发验证码
    async fn login_phone(&mut self, phone: &str) -> anyhow::Result<LoginPhoneResponse> {
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        self.trace_html("login_phone_1_after_goto");

        // 水合慢时登录按钮可能未渲染，等 3s 复检
        if self.is_logged_in() {
            self.page.settle(3000).await;
            if self.is_logged_in() {
                anyhow::bail!("已登录，无需再次登录");
            }
        }

        // 点「登录」打开 passport 弹窗（iframe）
        let res = self
            .page
            .evaluate(CLICK_LOGIN_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if !res.starts_with("CLICKED:") {
            tracing::warn!("[qianwen] 未找到「登录」按钮（{res}）");
        }
        self.page.settle(2000).await;
        self.trace_html("login_phone_2_after_click_login");

        // 等 passport iframe 出现（最多 10s）
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.passport_frame_index().is_none() {
            if Instant::now() > deadline {
                self.trace_html("login_phone_3_no_passport_frame");
                anyhow::bail!(
                    "登录弹窗未出现（无 passport iframe）。可 GET /api/qianwen/debug 查看页面状态"
                );
            }
            self.page.settle(1000).await;
        }
        self.page.settle(2000).await;

        // 填手机号
        let phone_json = serde_json::to_string(phone)?;
        let fill_js = format!("{FRAME_FILL_PHONE_JS}({phone_json})");
        let fill_result = self
            .eval_in_passport(&fill_js)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "eval-failed".into());
        if fill_result != "ok" {
            self.trace_html("login_phone_4_fill_phone_failed");
            anyhow::bail!("手机号填入失败（{fill_result}），passport 表单结构可能已变化");
        }
        self.page.settle(1000).await;
        self.trace_html("login_phone_5_after_fill_phone");

        // 勾选用户协议（默认未勾选，不勾无法发送验证码/登录）
        let _ = self.eval_in_passport(FRAME_CHECK_AGREEMENT_JS);
        self.page.settle(500).await;

        // 点击「获取验证码」
        let sent = self
            .eval_in_passport(FRAME_SEND_CODE_JS)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "eval-failed".into());
        if sent != "clicked" {
            self.trace_html("login_phone_6_send_code_failed");
            anyhow::bail!("点击「获取验证码」失败（{sent}）");
        }
        self.page.settle(1500).await;

        // 风控可能弹图片验证码（#fm-login-checkcode 变可见）
        let captcha = self
            .eval_in_passport(FRAME_CAPTCHA_JS)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.trace_html("login_phone_7_after_send_code");
        if captcha {
            anyhow::bail!(
                "触发图片验证码（风控），当前版本不支持自动识别。\
                 请稍后重试，或换网络环境/降低登录频率"
            );
        }

        Ok(LoginPhoneResponse {
            tip: "验证码已发送，请调用 POST /api/qianwen/login/verify 提交验证码".into(),
        })
    }

    /// 短信登录第二步：填验证码 → 点登录 → 等待 topRedirect 跳回 chat 完成登录
    async fn login_verify(&mut self, code: &str) -> anyhow::Result<LoginVerifyResponse> {
        let code_json = serde_json::to_string(code)?;
        let fill_js = format!("{FRAME_FILL_CODE_JS}({code_json})");
        let fill_result = self
            .eval_in_passport(&fill_js)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "eval-failed".into());
        if fill_result != "ok" {
            self.trace_html("login_verify_1_fill_code_failed");
            anyhow::bail!("验证码填入失败（{fill_result}），登录弹窗可能已关闭，请重新 POST /login/phone");
        }
        self.page.settle(500).await;
        self.trace_html("login_verify_2_after_fill_code");

        // 确认协议已勾选（弹窗重开等场景）
        let _ = self.eval_in_passport(FRAME_CHECK_AGREEMENT_JS);
        self.page.settle(300).await;

        // 点「登录」
        let submit = self
            .eval_in_passport(FRAME_SUBMIT_JS)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "eval-failed".into());
        if submit != "clicked" {
            anyhow::bail!("点击登录按钮失败（{submit}）");
        }
        self.page.settle(3000).await;
        self.trace_html("login_verify_3_after_submit");

        // topRedirect 跳回 chat 后登录按钮消失；最多 20s
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if self.is_logged_in() {
                self.save_cookies();
                return Ok(LoginVerifyResponse { logged_in: true });
            }
            if Instant::now() > deadline {
                self.trace_html("login_verify_4_timeout");
                return Ok(LoginVerifyResponse { logged_in: false });
            }
            self.page.settle(1000).await;
        }
    }

    // ---- 提问 ----

    /// 填入问题并点击发送。发送后校验：输入框清空 或 出现新一轮 .chat-round。
    async fn send_message(&mut self, text: &str) -> anyhow::Result<()> {
        self.enable_console_capture();
        let fill_js = build_fill_js(text)?;
        let fill_result = self
            .page
            .evaluate(&fill_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        tracing::info!("[qianwen] fill 结果: {fill_result}");
        if fill_result == "no-input" {
            anyhow::bail!("找不到输入框（contenteditable/textarea）{}", self.console_tail(5));
        }
        // Slate 编辑器（data-slate-editor）：填入走 Slate 内部模型（fiber
        // insertText 或 beforeinput），渲染由 React setState 异步完成，fill
        // 阶段无法同步校验——等渲染后校验文本已写入且发送按钮已启用（Slate
        // 状态同步后按钮才会从 disabled 解锁），失败则明确报错而非盲目点发送
        if fill_result == "slate-pending" {
            self.page.settle(1200).await;
            let slate_result = self
                .page
                .evaluate(&slate_check_js(text)?)
                .as_str()
                .unwrap_or("eval-failed")
                .to_string();
            tracing::info!("[qianwen] Slate 渲染后校验: {slate_result}");
            match slate_result.as_str() {
                "ok" => {}
                s if s.starts_with("button-disabled") => {
                    self.trace_html("ask_slate_button_disabled");
                    let snapshot = self.dump_html();
                    anyhow::bail!(
                        "文本已写入但发送按钮未激活（Slate 状态未同步，{slate_result}）。\
                         页面快照已保存: {snapshot}{}",
                        self.console_tail(5)
                    );
                }
                _ => {
                    self.trace_html("ask_slate_no_text");
                    let snapshot = self.dump_html();
                    anyhow::bail!(
                        "Slate 编辑器写入未生效（{slate_result}）。\
                         页面快照已保存: {snapshot}{}",
                        self.console_tail(5)
                    );
                }
            }
        } else if !fill_result.starts_with("ok-") {
            anyhow::bail!("问题文本未写入输入框（{fill_result}）{}", self.console_tail(5));
        }
        self.page.settle(1000).await;
        self.trace_html("ask_1_after_fill");

        let rounds_before = self
            .page
            .evaluate("document.querySelectorAll('.chat-round').length")
            .as_u64()
            .unwrap_or(0);

        // 发送：优先 aria-label 按钮；退化为 Enter 键
        let sent = self
            .page
            .evaluate(SEND_CLICK_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        tracing::info!("[qianwen] 发送方式: {sent}（发送前轮次 {rounds_before}）");
        if sent == "none" {
            anyhow::bail!("发送失败：既无发送按钮也无输入框{}", self.console_tail(5));
        }
        self.page.settle(1000).await;
        self.trace_html("ask_2_after_send");

        // 发送结果校验：输入框仍含问题文本 且 无新增 .chat-round 视为未生效。
        // doubao/deepseek 有"重试一次"、kimi 有"再等一次"，qianwen 同样补齐：
        // 先等 1.5s 复检（渲染慢），再重发一次（React 状态未同步时首击可能无效），
        // 仍失败才报错并自动落盘页面快照供排查
        let not_effective_js = format!(
            r#"((t, before) => {{
  const el = document.querySelector('[contenteditable="true"], textarea');
  const hasText = !!el && (el.textContent || el.value || '').trim().includes(t);
  const rounds = document.querySelectorAll('.chat-round').length;
  return hasText && rounds <= before;
}})({}, {})"#,
            serde_json::to_string(text)?,
            rounds_before
        );
        let mut not_effective = self
            .page
            .evaluate(&not_effective_js)
            .as_bool()
            .unwrap_or(true);
        tracing::info!("[qianwen] 首次发送后校验 not_effective={not_effective}");
        if not_effective {
            tracing::warn!("[qianwen] 首次发送未生效，等 1.5s 复检（可能渲染慢）");
            self.page.settle(1500).await;
            not_effective = self
                .page
                .evaluate(&not_effective_js)
                .as_bool()
                .unwrap_or(true);
            tracing::info!("[qianwen] 复检 not_effective={not_effective}");
        }
        if not_effective {
            tracing::warn!("[qianwen] 复检仍未生效，重试发送一次");
            let resent = self
                .page
                .evaluate(SEND_CLICK_JS)
                .as_str()
                .unwrap_or("")
                .to_string();
            tracing::info!("[qianwen] 重试发送方式: {resent}");
            self.page.settle(1500).await;
            not_effective = self
                .page
                .evaluate(&not_effective_js)
                .as_bool()
                .unwrap_or(true);
            tracing::info!("[qianwen] 重试后校验 not_effective={not_effective}");
        }
        if not_effective {
            self.trace_html("ask_3_send_not_effective");
            let snapshot = self.dump_html();
            anyhow::bail!(
                "发送未生效（输入框未清空且无新消息轮次），可能触发风控验证码。\
                 页面快照已保存: {snapshot}；\
                 可 GET /api/web-providers/qianwen/debug 查看页面状态，\
                 或 POST /api/web-providers/qianwen/reset 后重试{}",
                self.console_tail(8)
            );
        }
        Ok(())
    }

    /// 等回复说完；流式模式通过 chunk_tx 下发增量。
    /// 三阶段：宽限等网络在途 → 网络在途时死等捕获 → DOM 稳定性兜底（过滤问题回显）。
    async fn wait_reply(
        &mut self,
        question: &str,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        let start = Instant::now();

        // 阶段1：宽限 10s，等捕获或 completion 请求发出
        let mut engaged = false;
        while start.elapsed() < Duration::from_secs(10) {
            if self.captcha_present() {
                tracing::warn!("[qianwen] 检测到 baxia 风控滑块弹窗，快速失败");
                return Err(self.captcha_error());
            }
            let captured = self.reply_capture.lock().unwrap().clone();
            if let Some((c, _)) = captured {
                return self.finish_with_capture(c, chunk_tx).await;
            }
            match *self.completion_state.lock().unwrap() {
                CompletionState::InFlight | CompletionState::Done => {
                    engaged = true;
                    break;
                }
                CompletionState::None => {}
            }
            self.page.settle(500).await;
        }
        tracing::info!(
            "[qianwen] wait_reply 阶段1 结束：completion engaged={engaged}，网络捕获={}",
            self.reply_capture.lock().unwrap().is_some()
        );

        // 阶段2：completion 在途/已到达，以网络层为准等待捕获
        if engaged {
            let deadline = start + Duration::from_millis(self.cfg.max_wait_ms);
            while Instant::now() < deadline {
                if self.captcha_present() {
                    tracing::warn!("[qianwen] 检测到 baxia 风控滑块弹窗，快速失败");
                    return Err(self.captcha_error());
                }
                let captured = self.reply_capture.lock().unwrap().clone();
                if let Some((c, _)) = captured {
                    return self.finish_with_capture(c, chunk_tx).await;
                }
                self.page.settle(500).await;
            }
            // 网络超时无捕获 → 落到 DOM 兜底（可能 SSE 格式未覆盖）
            tracing::warn!("[qianwen] completion 已发出但未捕获到 SSE 回复，转 DOM 兜底");
        }

        // 阶段3：DOM 稳定性等待（过滤问题回显）
        let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let q_norm = norm(question);
        let dom_reply = |s: &str| -> String {
            let t = s.trim();
            if t.is_empty() || norm(t) == q_norm {
                String::new()
            } else {
                t.to_string()
            }
        };

        let mut last = dom_reply(&self.read_last());
        tracing::info!(
            "[qianwen] wait_reply 阶段3 DOM 兜底：read_last 首读 {} 字符",
            last.chars().count()
        );
        // DOM 增量是否已流出：捕获晚到时不再整段重发，避免流式内容重复
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
            if self.captcha_present() {
                tracing::warn!("[qianwen] 检测到 baxia 风控滑块弹窗，快速失败");
                return Err(self.captcha_error());
            }
            let captured = self.reply_capture.lock().unwrap().clone();
            if let Some((c, _)) = captured {
                let ctx = if streamed_any {
                    None
                } else {
                    chunk_tx.clone()
                };
                return self.finish_with_capture(c, ctx).await;
            }
            self.page.settle(1000).await;
            let cur = dom_reply(&self.read_last());
            if cur != last {
                if last.is_empty() && !cur.is_empty() {
                    tracing::info!(
                        "[qianwen] DOM 首次出现回复文本（{} 字符）",
                        truncate_utf8(&cur, 50).chars().count()
                    );
                }
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
            let stopping = self.is_stopping();
            if !stopping
                && !last.is_empty()
                && stable_since.elapsed().as_millis() as u64 >= self.cfg.stable_ms
            {
                break;
            }
            if Instant::now() > deadline {
                if last.is_empty() {
                    self.trace_html("wait_reply_timeout");
                    let snapshot = self.dump_html();
                    anyhow::bail!(
                        "{}s 内无回复（网络层与 DOM 均无内容）。\
                         页面快照已保存: {snapshot}{}",
                        self.cfg.max_wait_ms / 1000,
                        self.console_tail(8)
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

    async fn finish_with_capture(
        &mut self,
        captured: AiReply,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        if let Some(tx) = &chunk_tx {
            if !captured.thinking.is_empty() {
                let _ = tx
                    .send(encode_chunk(ChunkKind::Thinking, &captured.thinking))
                    .await;
            }
            if !captured.reply.is_empty() {
                let _ = tx.send(encode_chunk(ChunkKind::Text, &captured.reply)).await;
            }
        }
        self.trace_html("wait_3_net_reply");
        Ok(captured)
    }

    async fn do_ask(
        &mut self,
        question: String,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        // 千问 baxia 风控对 obscura 无头环境强制滑块（环境指纹判定，cookie
        // 转移无效、滑块组件在无头内加载超时无解），真 Chrome 不触发：
        // 本机装有 Chrome 时提问一律走 CDP Chrome（复用 CDP 授权 profile 的
        // 登录态与有效 x5sec）；obscura 路径仅作无 Chrome 环境的退化
        if discover_chrome().is_some() {
            // streamed：首次尝试是否已流出流式增量（重试防重复下发）
            let mut streamed = false;
            let mut res = self
                .ask_via_cdp(&question, chunk_tx.clone(), &mut streamed)
                .await;
            if let Err(e) = &res {
                // 会话已中断或僵死：关闭丢弃，保证下次 ask 全新启动
                if let Some(mut session) = self.cdp_chat.take() {
                    session.shutdown().await;
                }
                // 连接级失败（Chrome 窗口被关闭/调试连接中断）为瞬时故障：
                // 重启 Chrome 完整重试一次；已流出过增量则重试不流式（防重复）
                if e.to_string().starts_with("CDP 连接失败") {
                    tracing::warn!("[qianwen] CDP 提问失败：{e:#}；重启 Chrome 重试一次");
                    let retry_tx = if streamed { None } else { chunk_tx };
                    res = self.ask_via_cdp(&question, retry_tx, &mut streamed).await;
                    if res.is_err() {
                        if let Some(mut session) = self.cdp_chat.take() {
                            session.shutdown().await;
                        }
                    }
                }
            }
            return res;
        }
        if !self.is_logged_in() {
            anyhow::bail!(
                "{}尚未登录，请到「网页版账号」页对千问重新授权\
                 （CDP 浏览器登录；千问未登录提问会触发风控验证码）",
                crate::models::ERR_NOT_LOGGED_IN
            );
        }
        // 清除上一轮捕获，重新注册 on_response 回调
        *self.reply_capture.lock().unwrap() = None;
        *self.completion_state.lock().unwrap() = CompletionState::None;
        self.register_response_callback();
        // 关闭上一次可能残留的 baxia 风控弹窗（不改变风控状态，仅清理遮挡，
        // 让重试有机会走通）
        self.page.evaluate(
            r#"(() => {
  const c = document.querySelector('.baxia-dialog-close');
  if (c) c.click();
  return !!c;
})()"#,
        );
        self.send_message(&question).await?;
        let reply = self.wait_reply(&question, chunk_tx).await?;
        Ok(reply)
    }
}

// ---- CDP Chrome 提问通道 ----

impl Qianwen {
    /// CDP Chrome 提问：复用 CDP 授权的持久 profile（登录态与 x5sec 留存
    /// 其中）拉起可见的 Chrome 窗口完成提问。Chrome 窗口可见——若仍偶发
    /// baxia 风控，用户可在窗口中手动完成滑块，检测到弹窗消失后自动继续
    async fn ask_via_cdp(
        &mut self,
        question: &str,
        chunk_tx: Option<mpsc::Sender<String>>,
        streamed: &mut bool,
    ) -> anyhow::Result<AiReply> {
        // 1. 启动/复用会话（profile 被已打开的授权 Chrome 占用会启动失败）
        if self.cdp_chat.is_none() {
            tracing::info!("[qianwen] CDP 提问：启动 Chrome（复用授权 profile）");
            let url = self.cfg.base_url.trim_end_matches('/').to_string();
            let mut session = CdpChatSession::launch("qianwen", &url).await?;
            // 等页面就绪：输入框出现（登录态在 profile 中，通常直接就绪）
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let has_input = session
                    .evaluate(
                        r#"!!document.querySelector('[contenteditable="true"], textarea')"#,
                    )
                    .await?
                    .as_bool()
                    .unwrap_or(false);
                if has_input {
                    break;
                }
                if Instant::now() > deadline {
                    let _ = session.shutdown().await;
                    anyhow::bail!(
                        "CDP Chrome 页面 20s 内未出现输入框，profile 登录态可能失效，\
                         请到「网页版账号」页对千问重新走 CDP 授权"
                    );
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            self.cdp_chat = Some(session);
        }

        // 2. 填入（Slate fiber 策略与 obscura 路径同款 JS；Chrome 中 React
        //    渲染同样异步，slate-pending 需等渲染后校验）
        let fill_result = self.cdp_eval_str(&build_fill_js(question)?).await?;
        tracing::info!("[qianwen] CDP fill 结果: {fill_result}");
        match fill_result.as_str() {
            "no-input" => anyhow::bail!("CDP 页面找不到输入框（contenteditable/textarea）"),
            "slate-pending" => {
                tokio::time::sleep(Duration::from_millis(1200)).await;
                let slate_result = self.cdp_eval_str(&slate_check_js(question)?).await?;
                tracing::info!("[qianwen] CDP Slate 渲染后校验: {slate_result}");
                if slate_result != "ok" {
                    anyhow::bail!("CDP Slate 填入未生效（{slate_result}）");
                }
            }
            s if s.starts_with("ok-") => {}
            s => anyhow::bail!("CDP 问题文本未写入输入框（{s}）"),
        }

        // 3. 点击发送
        let rounds_before =
            self.cdp_eval_str("document.querySelectorAll('.chat-round').length")
                .await?;
        let sent = self.cdp_eval_str(SEND_CLICK_JS).await?;
        tracing::info!("[qianwen] CDP 发送方式: {sent}（发送前轮次 {rounds_before}）");
        if sent == "none" {
            anyhow::bail!("CDP 发送失败：既无发送按钮也无输入框");
        }

        // 4. 轮询 DOM 出流式增量（Chrome 页面自行运行，无需 obscura 的
        //    settle 事件泵），过滤问题回显；文本稳定且非生成中即完成
        let norm = |s: &str| {
            s.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        };
        let q_norm = norm(question);
        let dom_reply = |s: &str| -> String {
            let t = s.trim();
            if t.is_empty() || norm(t) == q_norm {
                String::new()
            } else {
                t.to_string()
            }
        };

        let start = Instant::now();
        let mut last = String::new();
        let mut stable_since = Instant::now();
        let deadline = start + Duration::from_millis(self.cfg.max_wait_ms);
        loop {
            // 偶发 baxia 滑块：Chrome 窗口可见，等用户手动完成（最多 120s）
            if self.cdp_eval_bool(CAPTCHA_PRESENT_JS).await? {
                tracing::warn!("[qianwen] CDP 模式触发 baxia 滑块，请在 Chrome 窗口手动完成验证");
                let captcha_deadline = Instant::now() + Duration::from_secs(120);
                loop {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    if Instant::now() > captcha_deadline {
                        anyhow::bail!("baxia 滑块 120s 内未完成，请在 Chrome 窗口完成验证后重试");
                    }
                    if !self.cdp_eval_bool(CAPTCHA_PRESENT_JS).await? {
                        tracing::info!("[qianwen] baxia 滑块已完成，继续等待回复");
                        break;
                    }
                }
                continue;
            }
            let cur = dom_reply(&self.cdp_eval_str(READ_LAST_JS).await?);
            if cur != last {
                if let Some(tx) = &chunk_tx {
                    let delta = if last.is_empty() || cur.starts_with(&last) {
                        cur[last.len()..].to_string()
                    } else {
                        cur.clone()
                    };
                    let _ = tx.send(encode_chunk(ChunkKind::Text, &delta)).await;
                    *streamed = true;
                }
                last = cur;
                stable_since = Instant::now();
            }
            let stopping = self.cdp_eval_bool(STOPPING_JS).await?;
            if !stopping
                && !last.is_empty()
                && stable_since.elapsed().as_millis() as u64 >= self.cfg.stable_ms
            {
                break;
            }
            if Instant::now() > deadline {
                if last.is_empty() {
                    anyhow::bail!(
                        "{}s 内无回复（CDP Chrome 页面无内容），请查看 Chrome 窗口状态",
                        self.cfg.max_wait_ms / 1000
                    );
                }
                break;
            }
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        tracing::info!(
            "[qianwen] CDP 回复完成（{} 字符）",
            last.chars().count()
        );
        Ok(AiReply {
            thinking: String::new(),
            reply: last,
            ..Default::default()
        })
    }

    /// 在 CDP 会话页面执行 JS 并以字符串返回结果（会话须已启动）
    async fn cdp_eval_str(&mut self, js: &str) -> anyhow::Result<String> {
        let session = self
            .cdp_chat
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("CDP 会话未启动"))?;
        Ok(session
            .evaluate(js)
            .await?
            .as_str()
            .unwrap_or("")
            .to_string())
    }

    /// 在 CDP 会话页面执行 JS 并以 bool 返回结果（求值失败按 false 处理）
    async fn cdp_eval_bool(&mut self, js: &str) -> anyhow::Result<bool> {
        let session = self
            .cdp_chat
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("CDP 会话未启动"))?;
        Ok(session.evaluate(js).await?.as_bool().unwrap_or(false))
    }
}

// ---- 千问 SSE 解析 ----

/// 从 SSE 流式响应体中提取 AI 回复（宽松解析，覆盖多种格式变体）：
/// - 直接字段：{"content":"增量","reasoning_content":"思考增量"}
/// - OpenAI 风格：{"choices":[{"delta":{"content":"..."}}]}
/// - data: [DONE] 结束标记跳过
/// - 搜索引用（字段名待登录后 dump 核对）：search_results/searchResults/references
fn extract_sse_reply(body: &str) -> Option<AiReply> {
    let mut reply = String::new();
    let mut thinking = String::new();
    let mut refs: Vec<Reference> = Vec::new();

    for line in body.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let data = line[5..].trim();
        if data == "[DONE]" || data.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };

        // 直接字段格式
        if let Some(c) = v.get("content").and_then(|c| c.as_str()) {
            if !c.is_empty() {
                reply.push_str(c);
            }
        }
        if let Some(rc) = v.get("reasoning_content").and_then(|c| c.as_str()) {
            if !rc.is_empty() {
                thinking.push_str(rc);
            }
        }

        // OpenAI choices 格式
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                let delta = choice.get("delta").or_else(|| choice.get("message"));
                if let Some(d) = delta {
                    if let Some(c) = d.get("content").and_then(|c| c.as_str()) {
                        if !c.is_empty() {
                            reply.push_str(c);
                        }
                    }
                    if let Some(rc) = d.get("reasoning_content").and_then(|c| c.as_str()) {
                        if !rc.is_empty() {
                            thinking.push_str(rc);
                        }
                    }
                }
            }
        }

        // 搜索引用（宽松提取，字段名待真实 dump 后核对）
        let search_arr = v
            .get("search_results")
            .or_else(|| v.get("searchResults"))
            .or_else(|| v.get("references"));
        if let Some(arr) = search_arr.and_then(|a| a.as_array()) {
            for item in arr {
                let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                let url = item
                    .get("url")
                    .or_else(|| item.get("link"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                if !title.is_empty() && !url.is_empty() && !refs.iter().any(|r| r.url == url) {
                    refs.push(Reference {
                        title: title.to_string(),
                        url: url.to_string(),
                    });
                }
            }
        }
    }

    let reply = reply.trim().to_string();
    let thinking = thinking.trim().to_string();
    if reply.is_empty() && thinking.is_empty() && refs.is_empty() {
        None
    } else {
        Some(AiReply {
            thinking,
            reply,
            references: refs,
            ..Default::default()
        })
    }
}
