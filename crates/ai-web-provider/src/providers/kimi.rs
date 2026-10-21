//! Kimi（www.kimi.com）平台 provider：短信登录、ask（思考/答案分离）、
//! cookie 持久化免登录、网络层 AI 回复捕获（SSE ground truth）。
//!
//! 站点逆向结论（2026-10，agent-browser 实测）：
//! - 未登录渲染完整聊天 UI，但点发送会弹登录框（登录弹窗 .login-modal-next-mask）
//! - 登录为短信验证码：.phone-form__phone-input（手机号，type=tel）/
//!   .phone-form__code-input（验证码）/ .phone-form__send（发送验证码按钮，
//!   手机号有效才 enabled）/ 弹窗内 button.km-button-primary（登录提交）
//! - 聊天编辑器：.chat-input-editor（Vue 自研 contenteditable，非 ProseMirror）；
//!   直接改 textContent 不触发框架状态，**paste ClipboardEvent 策略有效**，
//!   但 DOM 异步更新，填入后需 settle 再校验
//! - 发送按钮：div.send-button-container（div 非 button，内含 svg.send-icon）
//! - 聊天 API 请求路径含 /chat/completion（SSE 格式登录后 dump 到 html/ 核对）

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use obscura::{Browser, InterceptResolution};
use tokio::sync::mpsc;

use crate::common::chunk::{encode_chunk, ChunkKind};
use crate::common::cookie_import::parse_cookie_import;
use crate::common::qr::{qr_svg_data_url, save_qr_image};
use crate::common::util::{debug_html_dir, trace_page_html, truncate_utf8};
use crate::config::ProviderCfg;
use crate::models::*;
use crate::providers::Provider;

pub struct KimiProvider;

impl Provider for KimiProvider {
    const NAME: &'static str = "kimi";
    const DISPLAY: &'static str = "Kimi";
    const DEFAULT_URL: &'static str = "https://www.kimi.com/";
    const URL_ENV: &'static str = "KIMI_URL";

    async fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) {
        let browser = match Browser::builder()
            .stealth(true)
            .storage_dir(&cfg.storage_dir)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[kimi] obscura 启动失败: {e}");
                return;
            }
        };
        let page = match browser.new_page().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[kimi] 新建页面失败: {e}");
                return;
            }
        };
        Kimi::new(browser, page, cfg).run(rx).await;
    }
}

/// 聊天请求（/chat/completion）网络生命周期：
/// None → InFlight（请求已发出）→ Done（响应已完整到达）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionState {
    None,
    InFlight,
    Done,
}

pub struct Kimi {
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
    /// 微信扫码登录捕获：createLoginQRCode 返回的登录码 + 最近一次轮询状态
    qr_capture: Arc<Mutex<KimiQrCapture>>,
    /// 伪 volcano webId（真 SDK 在 obscura 内拿不到 device id，仅用于二维码 URL 拼接）
    device_id: String,
}

/// 网络层捕获的微信扫码登录状态（响应回调写入）
#[derive(Default)]
struct KimiQrCapture {
    /// CreateLoginQRCode 返回的登录码（qrLoginId）
    code: Option<String>,
    /// 最近一次 GetLoginQRCodeStatus 状态：PENDING / SCANNED / SUCCESS / EXPIRED
    status: Option<String>,
}

// ---- JS 片段 ----

/// 登录态：用户区头像带 not-login class 则未登录。
/// Kimi 匿名态也渲染完整聊天 UI（无「登录」按钮），不能以「无登录按钮」判已登录。
const LOGGED_IN_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  // 未登录标志：用户区头像/图标带 not-login class
  const notLogin = [...document.querySelectorAll('[class*="not-login"]')].find(vis);
  if (notLogin) return false;
  // 兜底：可见「登录」按钮 → 未登录
  const btn = [...document.querySelectorAll('button')]
    .find(el => vis(el) && (el.textContent || '').trim() === '登录');
  if (btn) return false;
  // 已登录标志：用户区容器可见且不含 not-login 头像
  const userArea = document.querySelector('.user-info-container, .user-area__main');
  return !!(userArea && vis(userArea));
})()"#;

/// 点击「登录」按钮（打开短信登录弹窗）。返回 'CLICKED:描述' / 'NONE'。
const CLICK_LOGIN_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const els = [...document.querySelectorAll('button')]
    .filter(el => vis(el) && (el.textContent || '').trim() === '登录');
  if (!els.length) return 'NONE';
  for (const t of ['pointerdown', 'mousedown', 'pointerup', 'mouseup']) {
    try {
      els[0].dispatchEvent(new MouseEvent(t, {bubbles: true, cancelable: true, view: window}));
    } catch (e) {}
  }
  els[0].click();
  return 'CLICKED:' + String(els[0].className || '').slice(0, 60);
})()"#;

/// 登录弹窗是否打开（.phone-form__phone-input 可见）
const LOGIN_MODAL_JS: &str = r#"(() => {
  const el = document.querySelector('.phone-form__phone-input');
  return !!(el && el.offsetWidth > 0);
})()"#;

/// 应用水合探测：.user-area 仅在客户端渲染后出现（SSR DOM 无头部用户区）。
/// statics.moonshot.cn 主 bundle ~2MB，经拦截代理加载偶发超过 obscura 3s 模块图
/// 超时；未水合时整页无事件监听（点击/填入全部无效），必须重载重试。
const HYDRATED_JS: &str = r#"(() => {
  return !!document.querySelector('.user-area, .user-info-container');
})()"#;

/// 在登录弹窗内填入手机号（Vue：native setter + input 事件即可）
const FILL_PHONE_JS: &str = r#"((phone) => {
  const el = document.querySelector('.phone-form__phone-input');
  if (!el) return 'no-input';
  el.focus();
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, phone);
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return el.value === phone ? 'ok' : 'fail';
})"#;

/// 勾选登录弹窗内的协议 checkbox（默认未勾选）
const CHECK_AGREEMENT_JS: &str = r#"(() => {
  const tel = document.querySelector('.phone-form__phone-input');
  const modal = tel ? tel.closest('[class*="login-modal"], [class*="loginModal"], [role="dialog"]') : null;
  const scope = modal || document;
  const cbs = [...scope.querySelectorAll('input[type="checkbox"]')]
    .filter(c => c.offsetWidth > 0 || c.offsetParent !== null);
  if (!cbs.length) return 'no-checkbox';
  for (const cb of cbs) {
    if (!cb.checked) cb.click();
  }
  return cbs.every(c => c.checked) ? 'checked' : 'unchecked';
})()"#;

/// 点击「发送验证码」（.phone-form__send，手机号有效才 enabled）
const SEND_CODE_JS: &str = r#"(() => {
  const btn = document.querySelector('.phone-form__send');
  if (!btn) return 'no-btn';
  if (btn.disabled) return 'disabled';
  btn.click();
  return 'clicked';
})()"#;

/// 在登录弹窗内填入验证码（.phone-form__code-input）
const FILL_CODE_JS: &str = r#"((code) => {
  const el = document.querySelector('.phone-form__code-input');
  if (!el) return 'no-input';
  el.focus();
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, code);
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return el.value === code ? 'ok' : 'fail';
})"#;

/// 点击登录弹窗内的「登录」提交按钮（弹窗内的 km-button-primary）
const SUBMIT_LOGIN_JS: &str = r#"(() => {
  const tel = document.querySelector('.phone-form__phone-input');
  const modal = tel ? tel.closest('[class*="login-modal"], [class*="loginModal"], [role="dialog"]') : null;
  const scope = modal || document;
  let btn = [...scope.querySelectorAll('button.km-button-primary')]
    .find(b => b.offsetWidth > 0 && !b.disabled);
  if (!btn) btn = [...scope.querySelectorAll('button')]
    .find(b => b.offsetWidth > 0 && !b.disabled && (b.textContent || '').trim() === '登录');
  if (!btn) return 'no-btn';
  btn.click();
  return 'clicked';
})()"#;

/// 读取最后一条 AI 回复文本（容器类名待登录后 trace 核对，先多级兜底）：
/// 1) Kimi 消息容器（chat-content/markdown/editor-content 类）
/// 2) 通用 markdown 渲染容器
/// 过滤操作按钮文本，避免「复制/重试/重新生成」混入。
const READ_LAST_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const clean = (node) => {
    const clone = node.cloneNode(true);
    clone.querySelectorAll('button, [class*="action"], [class*="toolbar"], [class*="operate"], [class*="feedback"], svg')
      .forEach(n => n.remove());
    return (clone.textContent || '').trim();
  };
  const pick = (sel) => {
    const els = [...document.querySelectorAll(sel)].filter(vis);
    for (let i = els.length - 1; i >= 0; i--) {
      // 输入框区域（chat-editor-content 包住 chat-input-editor 与 placeholder）不是 AI 回复
      if (els[i].closest('.chat-input-editor, .chat-editor-content, .chat-input-placeholder')) continue;
      const t = clean(els[i]);
      if (t.length > 2) return t;
    }
    return '';
  };
  let t = pick('[class*="chat-content"]');
  if (t) return t;
  t = pick('[class*="markdown"]');
  if (t) return t;
  return '';
})()"#;

/// 是否还在生成：可见的「停止」按钮（Kimi 生成中显示停止图标按钮）
const STOPPING_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const byText = [...document.querySelectorAll('button, [role="button"], div')]
    .some(el => vis(el) && /停止/.test((el.getAttribute('aria-label') || '') + (el.textContent || '').trim().slice(0, 6)));
  if (byText) return true;
  const stopSvg = document.querySelector('svg[name="Stop"], [class*="stop-button"], [class*="stopButton"]');
  return !!(stopSvg && vis(stopSvg.closest('button, [role="button"], div')));
})()"#;

/// 编辑器填入（paste 优先，input 事件兜底）：Kimi 的 Lexical/Vue 编辑器监听 paste。
/// obscura 运行时无 DataTransfer，用普通对象模拟 clipboardData。
/// DOM 异步更新，填入后需 settle 再校验（VERIFY_FILL_JS）。
const FILL_EDITOR_JS: &str = r#"((text) => {
  const ed = document.querySelector('.chat-input-editor');
  if (!ed) return 'no-input';
  ed.focus();
  // 先清空已有内容（selectAll + delete，再兜底清 innerHTML）
  try { document.execCommand('selectAll', false, null); } catch (e) {}
  try { document.execCommand('delete', false, null); } catch (e) {}
  if ((ed.textContent || '').trim() !== '') ed.innerHTML = '';
  // 构造 clipboardData：真浏览器用 DataTransfer；obscura 无 DataTransfer 用普通对象
  let cd;
  try {
    cd = new DataTransfer();
    cd.setData('text/plain', text);
  } catch (e) {
    cd = {
      types: ['text/plain'],
      files: [],
      items: [{
        kind: 'string', type: 'text/plain',
        getAsString: (cb) => cb(text)
      }],
      getData: (t) => (t === 'text/plain' || t === 'text' ? text : '')
    };
  }
  try {
    const ev = new ClipboardEvent('paste', { bubbles: true, cancelable: true });
    // obscura 的 ClipboardEvent 不存储 init.clipboardData，构造后直接赋值
    try { ev.clipboardData = cd; } catch (e2) {}
    ed.dispatchEvent(ev);
    return 'paste-dispatched';
  } catch (e) {}
  // 兜底：beforeinput/input 插入（同样需构造后赋值 inputType/data）
  try {
    const be = new InputEvent('beforeinput', { bubbles: true, cancelable: true });
    try { be.inputType = 'insertText'; be.data = text; } catch (e2) {}
    ed.dispatchEvent(be);
    const ie = new InputEvent('input', { bubbles: true });
    try { ie.inputType = 'insertText'; ie.data = text; } catch (e2) {}
    ed.dispatchEvent(ie);
    return 'input-dispatched';
  } catch (e) {}
  return 'dispatch-failed';
})"#;

/// 编辑器内容校验（paste 异步渲染后）
const VERIFY_FILL_JS: &str = r#"((text) => {
  const ed = document.querySelector('.chat-input-editor');
  if (!ed) return 'no-input';
  return (ed.textContent || '').includes(text) ? 'ok' : 'empty';
})"#;

/// 点击发送按钮（div.send-button-container，非 button 元素）
const CLICK_SEND_JS: &str = r#"(() => {
  const c = document.querySelector('div.send-button-container');
  if (!c) return 'no-send-btn';
  for (const t of ['pointerdown', 'mousedown', 'pointerup', 'mouseup']) {
    try {
      c.dispatchEvent(new MouseEvent(t, {bubbles: true, cancelable: true, view: window}));
    } catch (e) {}
  }
  c.click();
  return 'clicked';
})()"#;

/// 编辑器是否已清空（发送成功后 Kimi 清空编辑器）
const EDITOR_CLEARED_JS: &str = r#"(() => {
  const ed = document.querySelector('.chat-input-editor');
  if (!ed) return false;
  return (ed.textContent || '').trim() === '';
})()"#;

impl Kimi {
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
            qr_capture: Arc::new(Mutex::new(KimiQrCapture::default())),
            // volcano webId 形如 19 位数字；SDK 在 obscura 内初始化不完整拿不到真值，
            // 用时间戳伪随机生成一个稳定值即可（仅用于 /wechat/mp/auth 链接拼接）
            device_id: format!(
                "{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
                    % 9_000_000_000_000_000_000
                    + 1_000_000_000_000_000_000
            ),
        }
    }

    fn save_cookies(&self) {
        let Some(path) = &self.cfg.cookie_path else { return };
        match self.browser.cookies().save_to_file(path) {
            Ok(()) => {}
            Err(e) => tracing::warn!("[kimi] 保存 cookies 失败: {e}"),
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
        if let Err(e) = self.ensure_hydrated(2).await {
            tracing::warn!("[kimi] 启动水合未完成: {e}");
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
                    if let Err(e) = self.ensure_hydrated(2).await {
                        tracing::warn!("[kimi] 页面重建后水合未完成: {e}");
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
        let qr_capture = self.qr_capture.clone();
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
                // Kimi 聊天 API：/api/chat/completion
                let is_chat_api = url.contains("/chat/completion");

                if is_chat_api {
                    *completion_state.lock().unwrap() = CompletionState::Done;
                }

                // 微信扫码登录：捕获 CreateLoginQRCode 登录码与 GetLoginQRCodeStatus 轮询状态
                // （客户端在模块加载时缓存了原始 fetch 引用，页面 fetch 钩子抓不到，
                //   网络层 on_response 是唯一可靠的捕获点）
                if !is_stream && !is_chat_api {
                    if url.contains("AuthService/CreateLoginQRCode") {
                        let body = String::from_utf8_lossy(&resp.body);
                        if let Some(code) = extract_json_string_field(&body, "code") {
                            let mut cap = qr_capture.lock().unwrap();
                            cap.code = Some(code);
                            cap.status = Some("PENDING".into());
                        }
                    } else if url.contains("AuthService/GetLoginQRCodeStatus") {
                        let body = String::from_utf8_lossy(&resp.body);
                        for st in ["EXPIRED", "SCANNED", "SUCCESS", "PENDING"] {
                            if body.contains(st) {
                                let mut cap = qr_capture.lock().unwrap();
                                cap.status = Some(st.into());
                                break;
                            }
                        }
                    }
                }

                if !is_stream && !is_chat_api {
                    return;
                }

                let body_str = String::from_utf8_lossy(&resp.body);
                // SSE 聊天响应原文落盘（系统临时目录），便于核对 Kimi 协议
                if is_stream {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let path = debug_html_dir().join(format!("{ts}_kimi_sse.txt"));
                    if let Err(e) = std::fs::write(&path, body_str.as_bytes()) {
                        tracing::warn!("[kimi] SSE 响应落盘失败: {e}");
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
                    // 与 models.rs 约定一致：登录成功后扫码状态不再有意义（SUCCESS 残留会误报「待确认」）
                    qr_state: if logged { "none".to_string() } else { self.qr_state() },
                });
            }
            Command::LoginQr { reply_tx } => {
                let res = self.login_qr().await;
                let _ = reply_tx.send(res);
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
                let path = std::env::temp_dir().join(format!("kimi-screenshot-{ts}.html"));
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
                let val = self.page.evaluate(&js);
                let _ = reply_tx.send(val.to_string());
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
    /// （obscura 的 rustls 加载部分 CDN 失败，同豆包/千问处理）。
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
        if url_str.contains("/chat/completion") {
            let mut st = completion_state.lock().unwrap();
            if *st == CompletionState::None {
                *st = CompletionState::InFlight;
            }
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
        let is_kimi_cdn = url_str.contains("kimi.com") || url_str.contains("moonshot.cn");
        if !is_kimi_cdn {
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
                        tracing::warn!("[kimi] 代理读取body失败: {url_str} → {e}");
                        let _ = req.resolver.send(InterceptResolution::Fail {
                            reason: format!("proxy body read error: {e}"),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[kimi] 代理请求失败: {url_str} → {e}");
                let _ = req.resolver.send(InterceptResolution::Fail {
                    reason: format!("proxy error: {e}"),
                });
            }
        }
    }

    // ---- 基础状态 ----

    /// 严格 DOM 登录态判定（不含 cookie 兜底）。
    /// cookie 导入后的校验必须用它：注入的 cookie 本身就含 kimi_token，
    /// 带兜底会让无效 cookie 也报"导入成功"。
    fn dom_logged_in(&self) -> bool {
        self.page.evaluate(LOGGED_IN_JS).as_bool().unwrap_or(false)
    }

    fn is_logged_in(&self) -> bool {
        if self.dom_logged_in() {
            return true;
        }
        // cookie 兜底：moonshot passport 登录态（kimi_token/kps/kimi_mtoken）
        // 任一存在且非空。Kimi 匿名态也渲染完整聊天 UI，主 bundle（~2MB，
        // statics.moonshot.cn）经代理加载超时会导致未水合——SSR DOM 无
        // 用户区，DOM 判定持续假阴性；凭证在 jar 中即视为登录
        //（失效时站点会在响应中清除，届时 DOM 判定也会给出未登录）
        self.browser
            .cookies()
            .get_all()
            .iter()
            .any(|c| {
                matches!(c.name.as_str(), "kimi_token" | "kps" | "kimi_mtoken")
                    && !c.value.is_empty()
            })
    }

    /// 导入真实浏览器导出的 cookie 并校验登录态。
    /// CDP 授权（cdp_auth）拉起本地 Chrome 完成登录后，收集的 cookie
    /// 转成 EditThisCookie JSON 走这里注入；也兼容扩展手动导出的导入。
    /// 关键 cookie 是 `kimi_token` / `kps`（moonshot passport 登录态，
    /// www.kimi.com 域），候选还有 `kimi_mtoken`。
    async fn import_cookies(&mut self, raw: String) -> anyhow::Result<ImportCookiesResponse> {
        let entries = parse_cookie_import(&raw);
        if entries.is_empty() {
            anyhow::bail!(
                "未解析到任何 cookie。支持格式：\
                 1) EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组；\
                 2) 每行一条 `name=value; Domain=www.kimi.com; Path=/` Set-Cookie 串"
            );
        }
        let has_session = entries
            .iter()
            .any(|e| e.contains("kimi_token=") || e.contains("kps=") || e.contains("kimi_mtoken="));
        let mut imported = 0usize;
        for entry in &entries {
            match self.browser.cookies().set(entry, &self.cfg.base_url) {
                Ok(()) => imported += 1,
                Err(e) => tracing::warn!("[kimi] cookie 导入失败（{entry}）: {e}"),
            }
        }

        // 刷新页面验证登录态（登录态有效则出现对话输入框而非登录入口）。
        // goto 等待 load 事件：页面资源多时 30s 内 load 可能不触发——
        // 超时 ≠ DOM 不可用，先 settle 再判定；判定未登录且导航曾超时
        // 时重试一次（排除"页面没加载完导致假阴性"）
        let nav_err = self.page.goto(&self.cfg.base_url).await.err();
        if let Some(e) = &nav_err {
            tracing::warn!("[kimi] 导航等待 load 超时（{e}），页面可能已部分渲染，继续判定");
        }
        self.page.settle(3000).await;
        // 用严格 DOM 判定：注入的 cookie 本身就含 kimi_token，is_logged_in
        // 的 cookie 兜底会让无效 cookie 也误报"导入成功"
        let mut logged_in = self.dom_logged_in();
        if !logged_in && nav_err.is_some() {
            let _ = self.page.goto(&self.cfg.base_url).await;
            self.page.settle(3000).await;
            logged_in = self.dom_logged_in();
        }
        if logged_in {
            self.save_cookies();
        }
        let message = if logged_in {
            format!("导入 {imported} 条 cookie，登录态已恢复并持久化到 storage")
        } else if !has_session {
            format!(
                "导入 {imported} 条 cookie 但未登录：缺少关键 cookie `kimi_token`/`kps`\
                 （www.kimi.com 域）。请确认导出时已登录 Kimi 且导出的是\
                 www.kimi.com 的 cookie"
            )
        } else {
            format!(
                "导入 {imported} 条 cookie 但未登录：登录态可能已过期。\
                 请在浏览器重新登录 Kimi 后重试授权"
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

    fn debug_info(&self) -> String {
        let url = self
            .page
            .evaluate("location.href")
            .as_str()
            .unwrap_or("")
            .to_string();
        let logged = self.is_logged_in();
        let last = self.read_last();
        let login_modal = self
            .page
            .evaluate(LOGIN_MODAL_JS)
            .as_bool()
            .unwrap_or(false);
        format!(
            "{{\"url\":\"{url}\",\"logged_in\":{logged},\"login_modal\":{login_modal},\"last_reply\":\"{}\"}}",
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
        let path = std::env::temp_dir().join(format!("kimi-{ts}.html"));
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
        trace_page_html(&self.page, self.debug_html, &format!("kimi_{step}"));
    }

    // ---- 登录 ----

    /// 等待/重试直到 Kimi 应用水合（.user-area 客户端渲染后才出现）。
    /// 未水合时整页无事件监听（点登录/填编辑器全部无效），重载页面重试。
    async fn ensure_hydrated(&mut self, attempts: u32) -> anyhow::Result<()> {
        for i in 0..=attempts {
            if self
                .page
                .evaluate(HYDRATED_JS)
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(());
            }
            if i == attempts {
                break;
            }
            tracing::warn!("[kimi] 第{}次加载未水合（无 .user-area），重载重试", i + 1);
            self.page.goto(&self.cfg.base_url).await?;
            self.page.settle(4000).await;
        }
        self.trace_html("hydrate_failed");
        anyhow::bail!(
            "Kimi 页面多次加载后仍未水合（statics.moonshot.cn 主 bundle 超时），请重试"
        );
    }

    /// 短信登录第一步：导航 → 点登录 → 弹窗 → 填手机号 → 勾协议 → 发验证码
    async fn login_phone(&mut self, phone: &str) -> anyhow::Result<LoginPhoneResponse> {
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        self.ensure_hydrated(2).await?;
        self.trace_html("login_phone_1_after_goto");

        // 水合慢时登录按钮可能未渲染，等 3s 复检
        if self.is_logged_in() {
            self.page.settle(3000).await;
            if self.is_logged_in() {
                anyhow::bail!("已登录，无需再次登录");
            }
        }

        // 弹窗可能已经开着（未登录点发送等操作会自动弹出）
        self.open_login_modal("login_phone").await?;

        // 填手机号
        let phone_json = serde_json::to_string(phone)?;
        let fill_js = format!("{FILL_PHONE_JS}({phone_json})");
        let fill_result = self
            .page
            .evaluate(&fill_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        if fill_result != "ok" {
            self.trace_html("login_phone_4_fill_phone_failed");
            anyhow::bail!("手机号填入失败（{fill_result}），登录表单结构可能已变化");
        }
        self.page.settle(800).await;
        self.trace_html("login_phone_5_after_fill_phone");

        // 勾选协议（默认未勾选，不勾无法发送验证码）
        let _ = self.page.evaluate(CHECK_AGREEMENT_JS);
        self.page.settle(500).await;

        // 点击「发送验证码」
        let sent = self
            .page
            .evaluate(SEND_CODE_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if sent != "clicked" {
            self.trace_html("login_phone_6_send_code_failed");
            anyhow::bail!("点击「发送验证码」失败（{sent}）");
        }
        self.page.settle(1500).await;
        self.trace_html("login_phone_7_after_send_code");

        Ok(LoginPhoneResponse {
            tip: "验证码已发送，请调用 POST /api/kimi/login/verify 提交验证码".into(),
        })
    }

    /// 短信登录第二步：填验证码 → 点登录 → 等待登录成功
    async fn login_verify(&mut self, code: &str) -> anyhow::Result<LoginVerifyResponse> {
        let code_json = serde_json::to_string(code)?;
        let fill_js = format!("{FILL_CODE_JS}({code_json})");
        let fill_result = self
            .page
            .evaluate(&fill_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        if fill_result != "ok" {
            self.trace_html("login_verify_1_fill_code_failed");
            anyhow::bail!(
                "验证码填入失败（{fill_result}），登录弹窗可能已关闭，请重新 POST /login/phone"
            );
        }
        self.page.settle(800).await;
        self.trace_html("login_verify_2_after_fill_code");

        // 点击弹窗内「登录」提交
        let submit = self
            .page
            .evaluate(SUBMIT_LOGIN_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if submit != "clicked" {
            anyhow::bail!("点击登录按钮失败（{submit}）");
        }
        self.page.settle(3000).await;
        self.trace_html("login_verify_3_after_submit");

        // 登录成功：登录按钮消失/弹窗关闭；最多 20s
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

    /// 打开登录弹窗：优先点可见「登录」按钮；匿名态无按钮则填编辑器点发送触发。
    /// 弹窗已开着则直接返回。trace_prefix 用于 trace_html 落盘命名。
    async fn open_login_modal(&mut self, trace_prefix: &str) -> anyhow::Result<()> {
        let mut modal_open = self
            .page
            .evaluate(LOGIN_MODAL_JS)
            .as_bool()
            .unwrap_or(false);
        if !modal_open {
            let res = self
                .page
                .evaluate(CLICK_LOGIN_JS)
                .as_str()
                .unwrap_or("")
                .to_string();
            if !res.starts_with("CLICKED:") {
                // 匿名态 UI 常无「登录」按钮：RE 实测未登录点发送会弹出登录弹窗
                let trigger_js =
                    format!("{FILL_EDITOR_JS}({})", serde_json::to_string("hi")?);
                let _ = self
                    .page
                    .evaluate(&trigger_js)
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                self.page.settle(800).await;
                let verify = self
                    .page
                    .evaluate(&format!("{VERIFY_FILL_JS}({})", serde_json::to_string("hi")?))
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                if verify == "ok" {
                    let _ = self
                        .page
                        .evaluate(CLICK_SEND_JS)
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                }
            }
            self.page.settle(2000).await;
            // 等登录弹窗出现（最多 10s）
            let deadline = Instant::now() + Duration::from_secs(10);
            while !modal_open {
                modal_open = self
                    .page
                    .evaluate(LOGIN_MODAL_JS)
                    .as_bool()
                    .unwrap_or(false);
                if modal_open {
                    break;
                }
                if Instant::now() > deadline {
                    self.trace_html(&format!("{trace_prefix}_2_no_login_modal"));
                    anyhow::bail!(
                        "登录弹窗未出现（无 .phone-form__phone-input）。\
                         可 GET /api/kimi/debug 查看页面状态"
                    );
                }
                self.page.settle(1000).await;
            }
        }
        self.page.settle(1000).await;
        self.trace_html(&format!("{trace_prefix}_3_modal_open"));
        Ok(())
    }

    /// 微信扫码登录：打开登录弹窗（挂载微信扫码面板）→ 网络层捕获
    /// CreateLoginQRCode 登录码 → 本地渲染二维码（qrcodegen）。
    ///
    /// 页面自身的 GetLoginQRCodeStatus 轮询（1.2s）在用户微信确认后
    /// 自动完成登录，故弹窗必须保持打开、登录流程不关闭页面。
    /// 二维码内容与官方面板一致：/wechat/mp/auth?id=<code>&device_id=<webId>；
    /// volcano SDK 在 obscura 内拿不到真 webId，用进程启动时生成的伪 device_id。
    async fn login_qr(&mut self) -> anyhow::Result<QrResponse> {
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        self.ensure_hydrated(2).await?;
        if self.is_logged_in() {
            self.page.settle(3000).await;
            if self.is_logged_in() {
                anyhow::bail!("已登录，无需再次登录");
            }
        }

        self.open_login_modal("login_qr").await?;

        // 等网络层捕获登录码；状态 EXPIRED 时页面会自动刷新出新码，等新码可用
        let deadline = Instant::now() + Duration::from_secs(25);
        let code = loop {
            let cap = self.qr_capture.lock().unwrap();
            let usable = match (&cap.code, cap.status.as_deref()) {
                (Some(_), Some("EXPIRED")) => None, // 等页面刷新出的新码
                (Some(c), _) => Some(c.clone()),
                (None, _) => None,
            };
            drop(cap);
            if let Some(c) = usable {
                break c;
            }
            if Instant::now() > deadline {
                self.trace_html("login_qr_1_no_code");
                anyhow::bail!(
                    "未捕获到扫码登录码（CreateLoginQRCode 未发出或已过期未刷新），\
                     可重试 POST /api/kimi/login/qr"
                );
            }
            self.page.settle(1000).await;
        };

        let qr_content = format!(
            "https://www.kimi.com/wechat/mp/auth?id={code}&device_id={}",
            self.device_id
        );
        let data_url = qr_svg_data_url(&qr_content)?;
        let qr_file = save_qr_image(
            "Kimi",
            "kimi_login_qr",
            &data_url,
            "用微信扫码并在手机上确认登录",
        );
        Ok(QrResponse {
            qr_data_url: data_url,
            qr_file,
            tip: "用微信扫描二维码并确认登录，随后 GET /api/kimi/status 轮询登录状态"
                .into(),
        })
    }

    /// 扫码状态 → 网关状态机（ready/scanned/expired/none）
    fn qr_state(&self) -> String {
        let cap = self.qr_capture.lock().unwrap();
        match cap.status.as_deref() {
            Some("PENDING") => "ready".to_string(),
            Some("SCANNED") | Some("SUCCESS") => "scanned".to_string(),
            Some("EXPIRED") => "expired".to_string(),
            _ => "none".to_string(),
        }
    }

    // ---- 提问 ----

    /// 填入问题（paste 策略，DOM 异步更新需 settle 后校验并重试）并点击发送。
    /// 发送后校验：编辑器清空（Kimi 发送成功会清空编辑器）。
    async fn send_message(&mut self, text: &str) -> anyhow::Result<()> {
        let text_json = serde_json::to_string(text)?;
        let fill_js = format!("{FILL_EDITOR_JS}({text_json})");
        let fill_result = self
            .page
            .evaluate(&fill_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        if fill_result == "no-input" {
            anyhow::bail!("找不到聊天编辑器（.chat-input-editor）");
        }
        if fill_result != "paste-dispatched" && fill_result != "input-dispatched" {
            anyhow::bail!("编辑器填入调度失败（{fill_result}）");
        }
        // paste 后 Vue 异步渲染，settle 后校验，未生效重试一次
        self.page.settle(1000).await;
        let verify_js = format!("{VERIFY_FILL_JS}({text_json})");
        let mut verified = self
            .page
            .evaluate(&verify_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        if verified != "ok" {
            tracing::warn!("[kimi] 首次 paste 未生效（{verified}），重试一次");
            let _ = self.page.evaluate(&fill_js);
            self.page.settle(1500).await;
            verified = self
                .page
                .evaluate(&verify_js)
                .as_str()
                .unwrap_or("")
                .to_string();
            if verified != "ok" {
                self.trace_html("ask_1_fill_failed");
                anyhow::bail!("问题文本未写入编辑器（paste 策略未生效）");
            }
        }
        self.trace_html("ask_1_after_fill");

        // 点击发送按钮
        let sent = self
            .page
            .evaluate(CLICK_SEND_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if sent != "clicked" {
            anyhow::bail!("发送失败：未找到发送按钮（div.send-button-container）");
        }
        self.page.settle(1500).await;
        self.trace_html("ask_2_after_send");

        // 发送后校验：编辑器已清空（未清空可能是弹了登录框或风控验证码）
        let cleared = self
            .page
            .evaluate(EDITOR_CLEARED_JS)
            .as_bool()
            .unwrap_or(false);
        let login_modal = self
            .page
            .evaluate(LOGIN_MODAL_JS)
            .as_bool()
            .unwrap_or(false);
        if !cleared {
            if login_modal {
                self.trace_html("ask_3_login_modal_popped");
                anyhow::bail!("发送未生效：弹出了登录框（登录态可能已过期），请重新登录");
            }
            // 再等一次（可能渲染慢）
            self.page.settle(1500).await;
            let cleared2 = self
                .page
                .evaluate(EDITOR_CLEARED_JS)
                .as_bool()
                .unwrap_or(false);
            if !cleared2 {
                self.trace_html("ask_4_send_not_effective");
                anyhow::bail!(
                    "发送未生效（编辑器未清空），可能触发风控验证码。\
                     可 GET /api/kimi/debug 查看，或 POST /api/kimi/reset 后重试"
                );
            }
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

        // 阶段2：completion 在途/已到达，以网络层为准等待捕获
        if engaged {
            let deadline = start + Duration::from_millis(self.cfg.max_wait_ms);
            while Instant::now() < deadline {
                let captured = self.reply_capture.lock().unwrap().clone();
                if let Some((c, _)) = captured {
                    return self.finish_with_capture(c, chunk_tx).await;
                }
                self.page.settle(500).await;
            }
            // 网络超时无捕获 → 落到 DOM 兜底（可能 SSE 格式未覆盖）
            tracing::warn!("[kimi] completion 已发出但未捕获到 SSE 回复，转 DOM 兜底");
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
                    anyhow::bail!(
                        "{}s 内无回复（网络层与 DOM 均无内容）",
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
        if !self.is_logged_in() {
            anyhow::bail!(
                "{}尚未登录，请到「网页版账号」页对 Kimi 重新授权\
                 （CDP 浏览器登录；Kimi 未登录点发送会弹出登录框）",
                crate::models::ERR_NOT_LOGGED_IN
            );
        }
        // 清除上一轮捕获，重新注册 on_response 回调
        *self.reply_capture.lock().unwrap() = None;
        *self.completion_state.lock().unwrap() = CompletionState::None;
        self.register_response_callback();
        self.send_message(&question).await?;
        let reply = self.wait_reply(&question, chunk_tx).await?;
        Ok(reply)
    }
}

// ---- Kimi SSE 解析 ----

/// 从小 JSON 响应体中提取字符串字段值，如 {"code":"383bc302..."} 提取 code。
/// 网络层捕获的登录接口响应体很小，无需完整反序列化。
fn extract_json_string_field(body: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\":");
    let idx = body.find(&key)? + key.len();
    let rest = body[idx..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 从 SSE 流式响应体中提取 AI 回复（宽松解析，覆盖多种格式变体）：
/// - 直接字段：{"content":"增量","reasoning_content":"思考增量"}
/// - OpenAI 风格：{"choices":[{"delta":{"content":"..."}}]}
/// - data: [DONE] 结束标记跳过
fn extract_sse_reply(body: &str) -> Option<AiReply> {
    let mut reply = String::new();
    let mut thinking = String::new();

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
    }

    let reply = reply.trim().to_string();
    let thinking = thinking.trim().to_string();
    if reply.is_empty() && thinking.is_empty() {
        None
    } else {
        Some(AiReply {
            thinking,
            reply,
            ..Default::default()
        })
    }
}
