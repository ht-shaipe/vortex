//! DeepSeek 平台 provider：手机号登录、ask（思考过程/答案分离）、
//! cookie 持久化免登录、网络层 AI 回复捕获（SSE ground truth）。
//!
//! DeepSeek SSE 格式（JSON-Patch 流协议）:
//!   event: ready / title / close            → 元事件（title 的 content 是会话标题，非回复）
//!   data: {"v":{"response":{...,"fragments":[{"type":"RESPONSE","content":"..."}]}}}
//!                                            → 响应快照，fragments 按序含 THINKING/RESPONSE 片段
//!   data: {"p":"response/fragments/-1/content","o":"APPEND","v":"增量"}
//!   data: {"v":"续写"}                       → 无 p/o，沿用上一条补丁的目标
//!   data: {"p":"response/status","o":"SET","v":"FINISHED"}
//!
//! 兼容旧版直字段格式 {"content":..,"reasoning_content":..} / OpenAI choices 格式。

use std::sync::Arc;
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

pub struct DeepseekProvider;

impl Provider for DeepseekProvider {
    const NAME: &'static str = "deepseek";
    const DISPLAY: &'static str = "DeepSeek";
    const DEFAULT_URL: &'static str = "https://chat.deepseek.com/";
    const URL_ENV: &'static str = "DEEPSEEK_URL";

    async fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) {
        let browser = match Browser::builder()
            .stealth(true)
            .storage_dir(&cfg.storage_dir)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[deepseek] obscura 启动失败: {e}");
                return;
            }
        };
        let page = match browser.new_page().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[deepseek] 新建页面失败: {e}");
                return;
            }
        };
        let mut provider = Deepseek::new(browser, page, cfg);
        if provider.page.goto(&provider.cfg.base_url).await.is_ok() {
            provider.page.settle(3000).await;
        }
        provider.inject_worker_polyfill();
        provider.run(rx).await;
    }
}

/// /chat/completion 请求生命周期
#[derive(Clone, Copy, PartialEq)]
enum CompletionState {
    None,
    InFlight,
    Done,
}

/// 网络层捕获的一次完整回复：最终结果 + 原始增量序列（流式回放用）。
/// 增量按 SSE 到达顺序记录（含 thinking/text 分流），回放给流式客户端
/// 与最终 AiReply 内容一致（trim 差异除外）。
#[derive(Clone)]
struct CapturedReply {
    reply: AiReply,
    deltas: Vec<(ChunkKind, String)>,
}

pub struct Deepseek {
    browser: Browser,
    page: obscura::Page,
    cfg: Arc<ProviderCfg>,
    debug_html: bool,
    page_recreated: bool,
    cookie_tick: u32,
    reply_capture: Arc<std::sync::Mutex<Option<CapturedReply>>>,
    completion_state: Arc<std::sync::Mutex<CompletionState>>,
    response_cb_id: Option<u64>,
}

// ---- JS 片段 ----

/// 登录态：有可见"登录"按钮则未登录（优先——未登录首页也有 hero 输入框，
/// 不能用"有输入框"做首要判定）；其次有可见输入框则已登录；兜底 URL 含 /chat
const LOGGED_IN_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  // URL 在登录/注册页 → 未登录（优先级最高）。token 失效被前端登出时
  // 整页跳 /sign_in：登录页自带 contenteditable 输入框且无顶栏「登录」
  // 按钮，会被下面的输入框条件误判为已登录（实测踩过：
  // debug 显示 url=/sign_in 且 logged_in=true，ask 报"凭证失效"）
  if (/\/(sign_in|sign-in|login|signup|register)(\/|$)/i.test(location.pathname)) return false;
  // 有可见"登录"按钮 → 未登录（优先级最高）
  const loginBtn = [...document.querySelectorAll('button, a, [role="button"]')]
    .find(el => {
      if (!vis(el)) return false;
      const t = (el.textContent || '').trim();
      return t.length <= 8 && /^登录|^log\s*in/i.test(t);
    });
  if (loginBtn) return false;
  // 有可见输入框 → 已登录
  const inputs = [...document.querySelectorAll('[contenteditable="true"], textarea')];
  if (inputs.some(vis)) return true;
  // 兜底：URL 含 /chat 且无登录按钮 → 已登录
  return location.pathname.includes('/chat');
})()"#;

/// URL 是否在登录/注册路由（token 失效被前端整页登出的形态）。
/// [`Self::is_logged_in`] 的兜底前置检查：登录路由上 localStorage/cookie
/// 的残留凭证不可信——前端已经不认它（实测踩过：/sign_in 页
/// userToken 仍在 localStorage，兜底放行 ask，登录页上发送失败
/// 才报"凭证失效"，用户误以为授权没生效）
const LOGIN_ROUTE_JS: &str =
    r#"/\/(sign_in|sign-in|login|signup|register)(\/|$)/i.test(location.pathname)"#;

/// 读取最后一条 AI 回复文本
const READ_LAST_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const BAD = /AI-generated|for reference only|仅供参考|AI 生成|Check network and retry|网络异常|请重试/;
  // 1. DeepSeek 用 ds-message class 做消息容器，AI 回复在用户消息之后
  let msgs = [...document.querySelectorAll('.ds-message, [class*="ds-message"]')].filter(vis);
  if (msgs.length >= 2) {
    // 取最后一条（AI 回复），跳过只有错误文本的
    for (let i = msgs.length - 1; i >= 0; i--) {
      const text = (msgs[i].textContent || '').trim();
      if (text && !BAD.test(text) && text.length > 1) return text;
    }
  }
  // 2. 优先找 [role="assistant"] 或 data-role="assistant"
  let els = [...document.querySelectorAll('[role="assistant"], [data-role="assistant"]')].filter(vis);
  if (els.length) {
    const last = els[els.length - 1];
    let text = (last.textContent || '').trim();
    if (text && !BAD.test(text)) return text;
  }
  // 3. 找 markdown 渲染容器
  els = [...document.querySelectorAll('[class*="markdown"], [class*="message-content"], [class*="chat-message"], [class*="answer"], [class*="response"], .ds-collapsible-text')]
    .filter(vis)
    .filter(el => !BAD.test((el.textContent || '').trim()));
  if (els.length) {
    return (els[els.length - 1].textContent || '').trim();
  }
  // 4. 兜底：所有可见的 div，取最深的非空文本（排除错误/免责声明）
  const divs = [...document.querySelectorAll('div')]
    .filter(d => d.children.length === 0 && vis(d))
    .map(d => (d.textContent || '').trim())
    .filter(t => t.length > 5 && !BAD.test(t));
  return divs.length ? divs[divs.length - 1] : '';
})()"#;

/// 是否还在生成（"停止生成"按钮可见）
const STOPPING_JS: &str = r#"(() => {
  const b = [...document.querySelectorAll('button, [role="button"]')].find(b =>
    /停止|stop|abort/i.test(b.textContent || '') || /stop|abort/i.test(b.getAttribute('aria-label') || '')
  );
  return !!b;
})()"#;

/// 点击登录按钮
const CLICK_LOGIN_JS: &str = r#"(() => {
  const btn = [...document.querySelectorAll('button, a, [role="button"]')]
    .find(el => {
      if (el.offsetParent === null) return false;
      const t = (el.textContent || '').trim();
      return t.length <= 8 && /^登录|^log\s*in/i.test(t);
    });
  if (btn) { btn.click(); return 'CLICKED'; }
  return 'not-found';
})()"#;

/// 切换到手机号登录标签
const SWITCH_PHONE_TAB_JS: &str = r#"(() => {
  const tab = [...document.querySelectorAll('button, a, [role="tab"], [role="button"]')]
    .find(el => {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) return false;
      const t = (el.textContent || '').trim();
      return t.length <= 20 && /手机号|phone.*login|手机登录/i.test(t);
    });
  if (tab) { tab.click(); return true; }
  return false;
})()"#;

/// 填入手机号
const FILL_PHONE_JS: &str = r#"((phone) => {
  const all = [...document.querySelectorAll('input')];
  let target = all.find(el => /手机|phone/i.test(el.placeholder || ''));
  if (!target) target = all.find(el => el.type === 'tel' && /手机|phone/i.test(el.placeholder || ''));
  if (!target) target = all.find(el => el.type === 'tel');
  if (!target) target = all[0];
  if (!target) return false;
  target.focus();
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(target, phone);
  target.dispatchEvent(new InputEvent('input', {bubbles: true, inputType: 'insertText', data: phone}));
  target.dispatchEvent(new Event('change', {bubbles: true}));
  return true;
})"#;

/// 点击"发送验证码"按钮
const SEND_CODE_JS: &str = r#"(() => {
  const vis = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const btn = [...document.querySelectorAll('button, [role="button"]')]
    .find(el => {
      if (!vis(el)) return false;
      const t = (el.textContent || '').trim();
      return /发送验证码|获取验证码|send.*code/i.test(t);
    });
  if (btn) { btn.click(); return true; }
  return false;
})()"#;

/// 填入验证码
const FILL_CODE_JS: &str = r#"((code) => {
  const vis = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const inputs = [...document.querySelectorAll('input')].filter(vis);
  let target = inputs.find(el => /验证|code|otp/i.test(el.placeholder || ''));
  if (!target) target = inputs.find(el => el.getAttribute('autocomplete') === 'one-time-code');
  if (!target) target = inputs.find(el => el.maxLength <= 8 && el.type === 'tel');
  if (!target) return false;
  target.focus();
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(target, code);
  target.dispatchEvent(new InputEvent('input', {bubbles: true, inputType: 'insertText', data: code}));
  target.dispatchEvent(new Event('change', {bubbles: true}));
  return true;
})"#;

/// 点击"登录/确认"按钮（验证码填入后）
const CLICK_VERIFY_LOGIN_JS: &str = r#"(() => {
  const vis = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const btn = [...document.querySelectorAll('button, [role="button"]')]
    .find(el => {
      if (!vis(el)) return false;
      const t = (el.textContent || '').trim();
      return /^登录$|^确认$|^登\s*录$|^log\s*in$/i.test(t);
    });
  if (btn) { btn.click(); return true; }
  const submit = document.querySelector('button[type="submit"], [role="button"][type="submit"]');
  if (submit && vis(submit)) { submit.click(); return true; }
  return false;
})()"#;

/// Worker polyfill：obscura 的 Worker 环境缺 importScripts 且不支持 ES module worker，
/// 导致 DeepSeek PoW worker 创建失败（chat/completion 永远不发出）。
/// 方案：替换 window.Worker，在主线程 fetch worker 代码并用 Function 沙箱执行。
/// 关键点：
/// 1. obscura 的 XHR 忽略 sync 参数恒为异步（send 立即返回，status 尚为 0），
///    同步 importScripts 无法实现 → 缺 chunk 时抛 __needChunk 哨兵中断执行，
///    异步 fetch 预取后整体重跑 factory；chunk 注册在持久 selfObj 上，
///    重跑的 webpack runtime 通过 backlog forEach 幂等处理（node 沙箱已验证）；
/// 2. chunk 必须以 self=selfObj 的 Function 执行：全局 eval 会落到 window 上，
///    webpackChunk 注册错位导致 chunk 永远不被 runtime 处理；
/// 3. onmessage 是异步注册的（chunk 加载微任务后）→ 代码变换
///    `onmessage=` → `__powScope.onmessage=` 桥接到容器对象，postMessage 排队等就绪；
/// 4. webpack 用 `self.location + '/../../'` 推导 publicPath → self.location 必须是
///    String 包装对象（字符串化得 worker URL，属性亦可访问）；
/// 5. 真实挑战实测 wasm_solve 16ms 出解（salt 20 hex / expire_at 毫秒 / difficulty 144000）。
const WORKER_POLYFILL_JS: &str = r#"(() => {
  if (window.__powWorkerPatched) return 'already';
  window.__powWorkerPatched = true;

  const resolveUrl = (u, base) => {
    try { return new URL(u && u.href ? u.href : String(u), base || location.href).href; }
    catch (e) { return String(u); }
  };

  // worker 源码/chunk 缓存（每次 ask 重建 Worker 不再重复拉取 80KB JS）
  const codeCache = {};

  // onmessage 裸赋值发生在异步加载的模块里 → 代码变换桥接到 __powScope 容器
  const patchCode = (code) => code.replace(/(?<![.\w])onmessage\s*=/g, '__powScope.onmessage=');

  class FakeWorker {
    constructor(url) {
      this._onmessage = null;
      this._onerror = null;
      this._terminated = false;
      this._queue = [];
      this._scope = {};
      const self = this;
      const workerUrl = resolveUrl(url);

      // 异步预取 chunk；obscura 的 fetch 走拦截代理（reqwest native-tls）
      const prefetch = (u) => {
        if (codeCache[u]) return Promise.resolve(codeCache[u]);
        return fetch(u).then((r) => {
          if (!r.ok) throw new Error('HTTP ' + r.status + ' loading chunk ' + u);
          return r.text();
        }).then((t) => { codeCache[u] = t; return t; });
      };

      prefetch(workerUrl).then((code) => {
        if (self._terminated) return;
        // 模拟 worker 全局 self：location 用 String 包装（webpack 拼接 publicPath 依赖字符串化）
        const wu = new URL(workerUrl);
        const loc = new String(wu.href);
        loc.href = wu.href; loc.protocol = wu.protocol; loc.host = wu.host;
        loc.origin = wu.origin; loc.pathname = wu.pathname; loc.search = wu.search; loc.hash = wu.hash;
        // 持久 worker 全局 self：跨重试保留 webpackChunk 注册与全局状态
        const selfObj = self._selfObj || (self._selfObj = { location: loc });
        const scope = self._scope;
        const postOut = (msg) => {
          if (self._terminated) return;
          if (self._onmessage) { try { self._onmessage({ data: msg }); } catch (e) {} }
        };
        selfObj.postMessage = postOut;

        // 每个 chunk 只执行一次（模块注册幂等由 webpack backlog 保证）
        const evaluated = {};
        const importScriptsImpl = function () {
          for (let i = 0; i < arguments.length; i++) {
            const u = resolveUrl(arguments[i], workerUrl);
            if (!codeCache[u]) throw { __needChunk: u };
            if (evaluated[u]) continue;
            evaluated[u] = true;
            // chunk 里的 self 必须绑定 worker 作用域：全局 eval 会落到 window 上，
            // webpackChunk 注册错位导致 chunk 永远不被 runtime 处理
            const fn = new Function('self', 'postMessage', 'importScripts', '__powScope', patchCode(codeCache[u]));
            fn.call(selfObj, selfObj, postOut, importScriptsImpl, scope);
          }
        };

        // 缺 chunk → 抛 __needChunk 中断 → 预取后整体重跑 factory
        const runFactory = (attempt) => {
          try {
            const factory = new Function('self', 'postMessage', 'importScripts', '__powScope', patchCode(code));
            factory.call(selfObj, selfObj, postOut, importScriptsImpl, scope);
            self._ready = true;
            console.log('[FakeWorker] ready:', workerUrl);
            return true;
          } catch (e) {
            if (e && e.__needChunk && attempt < 12) {
              return prefetch(e.__needChunk).then(() => runFactory(attempt + 1));
            }
            throw e;
          }
        };
        return runFactory(0);
      }).catch((err) => {
        console.warn('[FakeWorker] load error:', err);
        const ev = { message: 'Worker error: ' + ((err && err.message) || err), error: err };
        if (self._onerror) { try { self._onerror(ev); } catch (e) {} }
        else if (self.onerror) { try { self.onerror(ev); } catch (e) {} }
      });
    }
    _dispatch(d, attempt) {
      const h = this._scope && this._scope.onmessage;
      if (typeof h !== 'function') return false;
      try { h({ data: d }); return true; }
      catch (e) {
        // handler 内按需 importScripts 缺 chunk：预取后重放该消息
        if (e && e.__needChunk && attempt < 12) {
          const u = e.__needChunk;
          fetch(u).then((r) => {
            if (!r.ok) throw new Error('HTTP ' + r.status + ' loading chunk ' + u);
            return r.text();
          }).then((t) => { codeCache[u] = t; this._dispatch(d, attempt + 1); })
            .catch((err) => console.warn('[FakeWorker] chunk prefetch failed:', err));
          return true;
        }
        console.warn('[FakeWorker] handler error:', e);
        if (this.onerror) { try { this.onerror(e); } catch (e2) {} }
        return true;
      }
    }
    postMessage(data) {
      if (this._terminated) return;
      if (this._ready && this._dispatch(data, 0)) return;
      // worker 代码/onmessage 尚未就绪：排队轮询（chunk 加载 + WASM init 为异步）
      this._queue.push(data);
      const self = this;
      const started = Date.now();
      const flush = () => {
        if (self._terminated) return;
        if (self._scope && typeof self._scope.onmessage === 'function') {
          const q = self._queue; self._queue = [];
          for (let i = 0; i < q.length; i++) self._dispatch(q[i], 0);
          return;
        }
        if (Date.now() - started < 30000) setTimeout(flush, 100);
        else {
          const err = new Error('FakeWorker onmessage not ready in 30s');
          console.warn('[FakeWorker]', err.message);
          if (self.onerror) { try { self.onerror(err); } catch (e) {} }
        }
      };
      setTimeout(flush, 100);
    }
    set onmessage(cb) { this._onmessage = cb; }
    get onmessage() { return this._onmessage; }
    set onerror(cb) { this._onerror = cb; }
    get onerror() { return this._onerror; }
    addEventListener(type, cb) {
      if (type === 'message') this._onmessage = cb;
      else if (type === 'error') this._onerror = cb;
    }
    removeEventListener(type) {
      if (type === 'message') this._onmessage = null;
      else if (type === 'error') this._onerror = null;
    }
    terminate() { this._terminated = true; this._queue = []; }
  }
  window.Worker = FakeWorker;
  return 'patched';
})()"#;

impl Deepseek {
    pub fn new(browser: Browser, page: obscura::Page, cfg: Arc<ProviderCfg>) -> Self {
        Self {
            browser,
            page,
            debug_html: cfg.debug_html,
            cfg,
            page_recreated: false,
            cookie_tick: 0,
            reply_capture: Arc::new(std::sync::Mutex::new(None)),
            completion_state: Arc::new(std::sync::Mutex::new(CompletionState::None)),
            response_cb_id: None,
        }
    }

    fn save_cookies(&self) {
        let Some(path) = &self.cfg.cookie_path else { return };
        match self.browser.cookies().save_to_file(path) {
            Ok(()) => {}
            Err(e) => tracing::warn!("[deepseek] 保存 cookies 失败: {e}"),
        }
    }

    /// 注入 Worker polyfill（幂等：页面内 __powWorkerPatched 检查）
    fn inject_worker_polyfill(&self) {
        let v = self
            .page
            .evaluate(WORKER_POLYFILL_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if v != "patched" && v != "already" {
            tracing::warn!("[deepseek] Worker polyfill 注入异常返回: {v:?}");
        }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let proxy_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // 须短于 CDN 服务端 keep-alive（~10s），避免复用已被关闭的空闲连接
            .pool_idle_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");

        let mut intercept_task = self.spawn_intercept_proxy(proxy_client.clone());

        let mut heartbeat = tokio::time::interval(Duration::from_millis(250));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if self.page_recreated {
                self.page_recreated = false;
                intercept_task.abort();
                intercept_task = self.spawn_intercept_proxy(proxy_client.clone());
                self.register_response_callback();
                if self.page.goto(&self.cfg.base_url).await.is_ok() {
                    self.page.settle(3000).await;
                }
                self.inject_worker_polyfill();
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
                // DeepSeek 聊天 API: /api/v0/chat/completion
                let is_chat_api = url.contains("/chat/completion")
                    || url.contains("/chat/completions");

                // /chat/completion 响应已完整到达
                if is_chat_api {
                    *completion_state.lock().unwrap() = CompletionState::Done;
                }

                if !is_stream && !is_chat_api {
                    return;
                }
                let body_str = String::from_utf8_lossy(&resp.body);
                // SSE 响应原文落盘（系统临时目录，便于分析协议结构）
                if is_stream {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let path = debug_html_dir().join(format!("deepseek_{ts}_sse.txt"));
                    if let Err(e) = std::fs::write(&path, body_str.as_bytes()) {
                        tracing::warn!("[deepseek] SSE 响应落盘失败: {e}");
                    }
                }
                if let Some(cap) = extract_sse_reply(&body_str) {
                    if !cap.reply.reply.is_empty() || !cap.reply.thinking.is_empty() {
                        *reply_capture.lock().unwrap() = Some(cap);
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
                    "DeepSeek 不支持二维码登录，请使用 POST /api/deepseek/login/phone + /login/verify"
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
                match self.browser.new_page().await {
                    Ok(p) => {
                        self.page = p;
                        self.page_recreated = true;
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
                let _ = reply_tx.send(self.screenshot());
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
                local_storage,
                reply_tx,
            } => {
                let resp = self.import_cookies(cookies, local_storage).await;
                let _ = reply_tx.send(resp);
            }
        }
    }

    /// 拦截 JS/CSS 请求用 reqwest(native-tls) 代理（同豆包，rustls 兼容性问题）
    async fn handle_intercept(
        req: obscura::InterceptedRequest,
        client: &reqwest::Client,
        completion_state: &Arc<std::sync::Mutex<CompletionState>>,
    ) {
        let rt = req.resource_type.as_str();
        let url_str = req.url.as_str();

        // /chat/completion 请求已发出：标记网络路径在途
        if url_str.contains("/chat/completion") || url_str.contains("/chat/completions") {
            let mut st = completion_state.lock().unwrap();
            if *st == CompletionState::None {
                *st = CompletionState::InFlight;
            }
        }

        // 代理 JS/CSS（rustls 兼容性）+ 验证码图片（fengkongcloud.cn）
        // + Worker polyfill 主线程 fetch/XHR 加载的 JS 与 WASM（PoW 依赖，
        //   resource_type 非 Script，需按后缀识别）
        let is_static_asset = url_str.ends_with(".js")
            || url_str.ends_with(".wasm")
            || url_str.contains(".js?")
            || url_str.contains(".wasm?");
        let need_proxy = rt == "Script"
            || rt == "Stylesheet"
            || is_static_asset
            || (rt == "Image" && url_str.contains("fengkongcloud"));
        if !need_proxy {
            let _ = req.resolver.send(InterceptResolution::Continue {
                url: None,
                method: None,
                headers: None,
                body: None,
            });
            return;
        }
        // 放宽 URL 过滤：只要不是明显无关的域名就放行代理
        if !url_str.contains("deepseek.com")
            && !url_str.contains("cdn")
            && !url_str.contains("fengkongcloud")
            && !url_str.contains("aliyun")
            && !url_str.contains("cloudflare")
            && !url_str.contains("jsdelivr")
            && !url_str.contains("unpkg")
            && !url_str.contains("static")
        {
            let _ = req.resolver.send(InterceptResolution::Continue {
                url: None,
                method: None,
                headers: None,
                body: None,
            });
            return;
        }
        let url = &req.url;
        let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
            .unwrap_or(reqwest::Method::GET);

        // 代理获取（静态资源 GET 幂等）：连接层偶发失败（服务端已关闭的
        // 空闲连接复用）时重试，失败的资源越多页面 load 越慢（导航超时）
        const PROXY_SEND_RETRIES: usize = 2;
        let mut resp_opt = None;
        for attempt in 0..=PROXY_SEND_RETRIES {
            match client.request(method.clone(), url).send().await {
                Ok(resp) => {
                    resp_opt = Some(resp);
                    break;
                }
                Err(e) if attempt < PROXY_SEND_RETRIES => {
                    tracing::warn!(
                        "[deepseek] 代理请求失败（将重试 {}/{}）: {url} → {e:#}",
                        attempt + 1,
                        PROXY_SEND_RETRIES
                    );
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                Err(e) => {
                    tracing::warn!("[deepseek] 代理请求失败: {url} → {e:#}");
                }
            }
        }

        let Some(resp) = resp_opt else {
            let _ = req.resolver.send(InterceptResolution::Fail {
                reason: "proxy request error (retries exhausted)".into(),
            });
            return;
        };
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
                tracing::warn!("[deepseek] 代理读取body失败: {url} → {e}");
                let _ = req.resolver.send(InterceptResolution::Fail {
                    reason: format!("proxy body read error: {e}"),
                });
            }
        }
    }

    fn is_logged_in(&self) -> bool {
        if self.dom_logged_in() {
            return true;
        }
        // URL 在登录路由 → 前端已被服务器端登出：下面 localStorage/cookie
        // 兜底里的残留凭证不可信（实测踩过：/sign_in 页 userToken 残留
        // 导致兜底放行 ask，登录页上发送失败才报"凭证失效"）。
        // 不在登录路由时兜底照常生效（登录弹窗遮蔽输入框的抖动场景
        // URL 仍在 /chat）
        if self
            .page
            .evaluate(LOGIN_ROUTE_JS)
            .as_bool()
            .unwrap_or(false)
        {
            return false;
        }
        // localStorage 兜底：现版前端把会话凭证 userToken 存于 localStorage
        // （不落 cookie），旧 cookie 兜底检测不到它。登录弹窗/加载过渡会
        // 短暂遮蔽输入框导致 DOM 判定抖动假阴性——页面在目标 origin 上且
        // userToken 非空即视为登录（token 无效时前端会在初始化中移除它，
        // 届时 DOM 判定同样给出未登录）
        let origin = serde_json::to_string(&Self::base_origin(&self.cfg.base_url))
            .unwrap_or_default();
        let ls_logged = self
            .page
            .evaluate(&format!(
                r#"((o) => {{
                  try {{
                    if (location.origin !== o) return false;
                    return !!(localStorage.getItem('userToken') || '').trim();
                  }} catch (e) {{ return false; }}
                }})({origin})"#
            ))
            .as_bool()
            .unwrap_or(false);
        if ls_logged {
            return true;
        }
        // 旧版 cookie 兜底：userToken 存在且非空（历史版本落 cookie）。
        // 限定平台域：其他站点的同名 cookie 不得误判登录态
        let host = Self::base_host(&self.cfg.base_url);
        self.browser
            .cookies()
            .get_all()
            .iter()
            .any(|c| {
                let d = c.domain.trim_start_matches('.');
                c.name == "userToken"
                    && !c.value.is_empty()
                    && (d == host || d.ends_with(&format!(".{host}")))
            })
    }

    /// 严格 DOM 登录态判定（不含 cookie 兜底）。
    /// cookie 导入后的校验必须用它：注入的 cookie 本身就含 userToken，
    /// 带兜底会让无效 cookie 也报"导入成功"。
    fn dom_logged_in(&self) -> bool {
        self.page.evaluate(LOGGED_IN_JS).as_bool().unwrap_or(false)
    }

    /// URL 的 origin（scheme://host[:port]，截掉 path 及以后）。
    /// localStorage 按 origin 隔离，注入前须确认页面已在目标 origin 上
    fn base_origin(url: &str) -> String {
        let rest = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"));
        match rest {
            Some(r) => {
                let end = r.find('/').unwrap_or(r.len());
                url[..url.len() - r.len()].to_string() + &r[..end]
            }
            None => url.to_string(),
        }
    }

    /// URL 的主机名（cookie 兜底域匹配用）
    fn base_host(url: &str) -> String {
        url.trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    }

    /// 导入真实浏览器导出的 cookie 并校验登录态。
    /// CDP 授权（cdp_auth）拉起本地 Chrome 完成登录后，收集的 cookie
    /// 转成 EditThisCookie JSON 走这里注入；也兼容扩展手动导出的导入。
    /// DeepSeek 现版会话凭证 `userToken` 存于 **localStorage**（不落
    /// cookie），CDP 授权会连同 localStorage 一并收集传入 `local_storage`，
    /// 注入页面后再导航让前端读取初始化。
    async fn import_cookies(
        &mut self,
        raw: String,
        local_storage: Option<std::collections::HashMap<String, String>>,
    ) -> anyhow::Result<ImportCookiesResponse> {
        let entries = parse_cookie_import(&raw);
        if entries.is_empty() {
            anyhow::bail!(
                "未解析到任何 cookie。支持格式：\
                 1) EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组；\
                 2) 每行一条 `name=value; Domain=chat.deepseek.com; Path=/` Set-Cookie 串"
            );
        }
        let has_session = entries.iter().any(|e| e.contains("userToken="));
        // localStorage 是否含有效 userToken（导入结果消息分流用）
        let ls_token = local_storage
            .as_ref()
            .map(|m| {
                m.get("userToken")
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        // localStorage 注入（userToken 等凭证在 localStorage，cookie 注入不恢复登录态）。
        // localStorage 按 origin 隔离：页面不在目标站时先导航过去再写入
        if let Some(ls) = local_storage.as_ref().filter(|m| !m.is_empty()) {
            let page_origin = self
                .page
                .evaluate("location.origin")
                .as_str()
                .unwrap_or("")
                .to_string();
            if page_origin != Self::base_origin(&self.cfg.base_url) {
                let _ = self.page.goto(&self.cfg.base_url).await;
            }
            for (k, v) in ls {
                let js = format!(
                    "localStorage.setItem({}, {})",
                    serde_json::to_string(k).unwrap_or_default(),
                    serde_json::to_string(v).unwrap_or_default(),
                );
                self.page.evaluate(&js);
            }
        }
        let mut imported = 0usize;
        for entry in &entries {
            match self.browser.cookies().set(entry, &self.cfg.base_url) {
                Ok(()) => imported += 1,
                Err(e) => tracing::warn!("[deepseek] cookie 导入失败（{entry}）: {e}"),
            }
        }

        // 刷新页面验证登录态（userToken 有效则无登录按钮且出现对话输入框；
        // 用严格 DOM 判定，cookie 兜底会让无效 cookie 也误报成功）。
        // goto 等待 load 事件：DeepSeek 页面资源多（风控/wasm/埋点），
        // 单线程引擎里 30s 内 load 可能不触发——超时 ≠ DOM 不可用，
        // 先 settle 再判定；判定未登录且导航曾超时时重试一次（排除
        // "页面没加载完导致假阴性"）。CDP 侧导入等待 120s，容得下两轮
        let nav_err = self.page.goto(&self.cfg.base_url).await.err();
        if let Some(e) = &nav_err {
            tracing::warn!("[deepseek] 导航等待 load 超时（{e}），页面可能已部分渲染，继续判定");
        }
        self.page.settle(3000).await;
        let mut logged_in = self.dom_logged_in();
        if !logged_in && nav_err.is_some() {
            let _ = self.page.goto(&self.cfg.base_url).await;
            self.page.settle(3000).await;
            logged_in = self.dom_logged_in();
        }
        if !logged_in {
            // 微信 fastLogin 收敛窗口：CDP 收集的 userToken 可能在引擎内
            // 无效（前端判定或风控），页面弹登录流程；注入的微信域 cookie
            // 会让登录组件自动完成快捷登录（qrconnectchecklogin → fastLogin
            // → callback 设新 userToken）。判定阴性时轮询复判等收敛。
            // fastLogin 成功后组件的 callback 跳转可能被引擎吞掉（无自发
            // 导航），新 cookie 落地但页面未刷新——每 3 次阴性重新 goto
            // 让前端带新 cookie 重新初始化
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut poll = 0usize;
            while Instant::now() < deadline {
                poll += 1;
                if poll % 3 == 0 {
                    let _ = self.page.goto(&self.cfg.base_url).await;
                }
                self.page.settle(2000).await;
                if self.dom_logged_in() {
                    logged_in = true;
                    break;
                }
            }
            if !logged_in {
                // 诊断：DeepSeek 前端可能以 localStorage（cookie 注入不恢复）
                // 为登录态判定依据——打出键名与 cookie 状态供定位
                let ls_keys = self
                    .page
                    .evaluate("JSON.stringify(Object.keys(localStorage))")
                    .as_str()
                    .unwrap_or("[]")
                    .to_string();
                let url_now = self
                    .page
                    .evaluate("location.href")
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let token_now = self
                    .browser
                    .cookies()
                    .get_all()
                    .iter()
                    .any(|c| c.name == "userToken" && !c.value.is_empty());
                tracing::warn!(
                    "[deepseek] 轮询后仍阴性：URL={url_now}，CookieStore userToken={token_now}，localStorage keys={ls_keys}"
                );
            }
        }
        if logged_in {
            // 确认复查：防"注入后短暂呈现登录态、初始化继续跳 /sign_in"
            // 的水合竞态误报授权成功（token 被前端拒绝时跳转可能晚于
            // 3s settle；复查翻转则按未登录处理走对应提示分支）
            self.page.settle(2500).await;
            if !self.dom_logged_in() {
                tracing::warn!(
                    "[deepseek] 授权校验复查翻转：初始判定已登录但复查未登录\
                     （token 被前端初始化拒绝，页面跳登录路由）"
                );
                logged_in = false;
            }
        }
        if logged_in {
            self.save_cookies();
        }
        let message = if logged_in {
            format!("导入 {imported} 条 cookie，登录态已恢复并持久化到 storage")
        } else if ls_token {
            format!(
                "导入 {imported} 条 cookie 与 localStorage（含 userToken）但未登录：\
                 token 可能已过期或被风控拒绝，请在 Chrome 重新登录后重试授权"
            )
        } else if !has_session {
            format!(
                "导入 {imported} 条 cookie 但未登录：缺少关键凭证 `userToken`\
                  （DeepSeek 现版存于 localStorage，CDP 授权自动收集；手动导入\
                  需确认导出时已登录 DeepSeek 且是 chat.deepseek.com 的数据）"
            )
        } else {
            format!(
                "导入 {imported} 条 cookie 但未登录：userToken 可能已过期。\
                 请在浏览器重新登录 DeepSeek 后重试授权"
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
        let captcha_js = r#"(() => {
          const img = document.querySelector('img.shumei_captcha_loaded_img_bg');
          if (!img) return null;
          const r = img.getBoundingClientRect();
          return JSON.stringify({x: r.x, y: r.y, w: r.width, h: r.height, naturalW: img.naturalWidth, naturalH: img.naturalHeight});
        })()"#;
        let captcha = self
            .page
            .evaluate(captcha_js)
            .as_str()
            .unwrap_or("null")
            .to_string();
        format!(
            "{{\"url\":\"{url}\",\"logged_in\":{logged},\"last_reply\":\"{}\",\"captcha\":{captcha}}}",
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
        let path = std::env::temp_dir().join(format!("deepseek-{ts}.html"));
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
        trace_page_html(&self.page, self.debug_html, &format!("deepseek_{step}"));
    }

    fn screenshot(&self) -> anyhow::Result<String> {
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
        let path = std::env::temp_dir().join(format!("deepseek-screenshot-{ts}.html"));
        std::fs::write(&path, html.as_bytes())?;
        let path_str = path.to_string_lossy().to_string();
        Ok(path_str)
    }

    /// 手机号登录第一步：导航 → 点登录 → 切手机号标签 → 填手机号 → 发验证码
    async fn login_phone(&mut self, phone: &str) -> anyhow::Result<LoginPhoneResponse> {
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        self.trace_html("login_phone_1_after_goto");

        if self.is_logged_in() {
            anyhow::bail!("已登录，无需再次登录");
        }

        // 检查手机号输入框是否已可见；若不可见则尝试点击登录按钮
        let phone_json = serde_json::to_string(phone)?;
        let check_js = format!(
            r#"(() => {{
              const vis = (el) => {{ const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; }};
              const inp = [...document.querySelectorAll('input')].find(el => vis(el) && /phone|手机/i.test(el.placeholder || ''));
              return !!inp;
            }})()"#
        );
        let phone_visible = self.page.evaluate(&check_js).as_bool().unwrap_or(false);

        if !phone_visible {
            let res = self
                .page
                .evaluate(CLICK_LOGIN_JS)
                .as_str()
                .unwrap_or("")
                .to_string();
            if res != "CLICKED" {
                tracing::warn!("[deepseek] 未找到登录按钮（{res}），可能弹窗已打开");
            }
            self.page.settle(2000).await;

            let switched = self
                .page
                .evaluate(SWITCH_PHONE_TAB_JS)
                .as_bool()
                .unwrap_or(false);
            if !switched {
                tracing::warn!("[deepseek] 未找到手机号登录标签，可能已在手机号登录页");
            }
            self.page.settle(1500).await;
        }
        self.trace_html("login_phone_2_before_fill");

        // 填入手机号
        let fill_js = format!("{FILL_PHONE_JS}({phone_json})");
        let filled = self.page.evaluate(&fill_js).as_bool().unwrap_or(false);
        if !filled {
            anyhow::bail!(
                "未找到手机号输入框。可 GET /api/deepseek/debug 查看页面状态，\
                 或 GET /api/deepseek/dump-html 导出 HTML 快照"
            );
        }
        self.page.settle(500).await;
        self.trace_html("login_phone_3_after_fill_phone");

        // 点击"发送验证码"
        let sent = self
            .page
            .evaluate(SEND_CODE_JS)
            .as_bool()
            .unwrap_or(false);
        if !sent {
            anyhow::bail!("未找到「发送验证码」按钮，可能手机号格式不对或按钮 disabled");
        }
        self.page.settle(500).await;
        self.trace_html("login_phone_4_after_send_code");

        Ok(LoginPhoneResponse {
            tip: "验证码已发送，请调用 POST /api/deepseek/login/verify 提交验证码".into(),
        })
    }

    /// 手机号登录第二步：填验证码 → 点登录 → 等待登录成功
    async fn login_verify(&mut self, code: &str) -> anyhow::Result<LoginVerifyResponse> {
        let code_json = serde_json::to_string(code)?;
        let fill_js = format!("{FILL_CODE_JS}({code_json})");
        let filled = self.page.evaluate(&fill_js).as_bool().unwrap_or(false);
        if !filled {
            anyhow::bail!("未找到验证码输入框");
        }
        self.page.settle(500).await;
        self.trace_html("login_verify_1_after_fill_code");

        // 点击登录/确认按钮
        let clicked = self
            .page
            .evaluate(CLICK_VERIFY_LOGIN_JS)
            .as_bool()
            .unwrap_or(false);
        if !clicked {
            // 可能填完验证码后自动登录，检查登录态
            tracing::warn!("[deepseek] 未找到登录按钮，检查是否已自动登录");
        }
        self.page.settle(3000).await;
        self.trace_html("login_verify_2_after_click_login");

        // 等待登录成功（最多 15 秒）
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if self.is_logged_in() {
                self.save_cookies();
                return Ok(LoginVerifyResponse { logged_in: true });
            }
            if Instant::now() > deadline {
                return Ok(LoginVerifyResponse { logged_in: false });
            }
            self.page.settle(1000).await;
        }
    }

    /// 填入问题并点击发送
    async fn send_message(&mut self, text: &str) -> anyhow::Result<()> {
        let fill_js = build_fill_js(text)?;
        let fill_result = self
            .page
            .evaluate(&fill_js)
            .as_str()
            .unwrap_or("")
            .to_string();
        if fill_result == "no-input" {
            anyhow::bail!("找不到输入框（contenteditable/textarea）");
        }
        if !fill_result.starts_with("ok-") {
            anyhow::bail!("问题文本未写入输入框（{fill_result}）");
        }
        self.page.settle(1000).await;
        self.trace_html("ask_1_after_fill");

        // 发送：优先找发送按钮（含 [role="button"]），退化为 Enter 键
        let sent = self
            .page
            .evaluate(
                r#"(() => {
  // 找发送按钮：aria-label / title / 文本含"发送"/"send"，含 [role="button"]
  let btn = [...document.querySelectorAll('button, [role="button"]')].find(b =>
    !b.disabled && (
      /发送|send/i.test(b.getAttribute('aria-label') || '') ||
      /发送|send/i.test(b.title || '') ||
      /发送|send/i.test(b.textContent || '')
    )
  );
  if (btn && !btn.disabled) { btn.click(); return 'button'; }
  // 退化为找输入框附近的可点击图标按钮（DeepSeek 发送按钮可能是图标）
  const input = document.querySelector('[contenteditable="true"], textarea');
  if (input) {
    const container = input.closest('form') || input.parentElement;
    if (container) {
      const btns = [...container.querySelectorAll('button, [role="button"]')].filter(b => !b.disabled);
      // 取最后一个可用的按钮（通常是发送按钮）
      if (btns.length) { btns[btns.length - 1].click(); return 'icon-button'; }
    }
  }
  // 退化为 Enter 键
  const el = document.querySelector('[contenteditable="true"], textarea');
  if (el) {
    el.focus();
    el.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keypress', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keyup', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}));
    return 'enter';
  }
  return 'none';
})()"#,
            )
            .as_str()
            .unwrap_or("")
            .to_string();
        if sent == "none" {
            anyhow::bail!("发送失败：既无发送按钮也无输入框");
        }
        self.page.settle(500).await;

        // 发送结果校验：发送成功会清空输入框；若问题文本仍在输入框则重试一次
        let input_still_has_text_js = format!(
            r#"((t) => {{
  const el = document.querySelector('[contenteditable="true"], textarea');
  if (!el) return false;
  return (el.textContent || el.value || '').trim().includes(t);
}})({})"#,
            serde_json::to_string(text)?
        );
        let mut not_sent = self
            .page
            .evaluate(&input_still_has_text_js)
            .as_bool()
            .unwrap_or(false);
        if not_sent {
            tracing::warn!("[deepseek] 首次发送未生效（输入框未清空），重试一次");
            self.page
                .evaluate(
                    r#"(() => {
  let btn = [...document.querySelectorAll('button, [role="button"]')].find(b =>
    !b.disabled && (
      /发送|send/i.test(b.getAttribute('aria-label') || '') ||
      /发送|send/i.test(b.title || '') ||
      /发送|send/i.test(b.textContent || '')
    )
  );
  if (btn && !btn.disabled) { btn.click(); return 'button'; }
  const el = document.querySelector('[contenteditable="true"], textarea');
  if (el) {
    el.focus();
    el.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keypress', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true, cancelable:true}));
    el.dispatchEvent(new KeyboardEvent('keyup', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}));
    return 'enter';
  }
  return 'none';
})()"#,
                );
            self.page.settle(800).await;
            not_sent = self
                .page
                .evaluate(&input_still_has_text_js)
                .as_bool()
                .unwrap_or(false);
        }
        if not_sent {
            anyhow::bail!("发送未生效（输入框文本未被发出），页面可能未完全加载或被风控");
        }
        self.trace_html("ask_2_after_send");
        Ok(())
    }

    /// DOM 路径的回复文本：read_last 并过滤掉问题回显
    fn dom_reply_text(&self, q_norm: &str) -> String {
        let dom = self.read_last();
        if dom.is_empty() {
            return String::new();
        }
        let dom_norm: String = dom.chars().filter(|c| !c.is_whitespace()).collect();
        if !q_norm.is_empty() && dom_norm == *q_norm {
            return String::new();
        }
        dom
    }

    /// 等回复说完；流式模式通过 chunk_tx 下发增量
    async fn wait_reply(
        &mut self,
        question: &str,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(self.cfg.max_wait_ms);
        let q_norm: String = question.chars().filter(|c| !c.is_whitespace()).collect();

        // 阶段1：宽限期内等 completion 请求发出；期间捕获若先到直接返回
        let mut network_engaged = false;
        while Instant::now() < deadline {
            let captured = self.reply_capture.lock().unwrap().clone();
            if let Some(cap) = captured {
                if !cap.reply.reply.is_empty() || !cap.reply.thinking.is_empty() {
                    return self.finish_with_capture(cap, chunk_tx, "ask_3_net_reply").await;
                }
            }
            let st = *self.completion_state.lock().unwrap();
            if st != CompletionState::None {
                network_engaged = true;
                break;
            }
            if start.elapsed() >= Duration::from_secs(10) {
                break;
            }
            self.page.settle(500).await;
        }

        // 阶段2：completion 在途，等网络层捕获
        if network_engaged {
            while Instant::now() < deadline {
                let captured = self.reply_capture.lock().unwrap().clone();
                if let Some(cap) = captured {
                    if !cap.reply.reply.is_empty() || !cap.reply.thinking.is_empty() {
                        return self.finish_with_capture(cap, chunk_tx, "ask_3_net_reply").await;
                    }
                }
                if *self.completion_state.lock().unwrap() == CompletionState::Done {
                    tracing::warn!("[deepseek] completion 响应已到达但网络层未提取到回复，退回 DOM 兜底");
                    break;
                }
                self.page.settle(500).await;
            }
        } else {
            tracing::warn!("[deepseek] 未检测到 completion 请求，退回 DOM 兜底");
        }

        // 阶段3：DOM 稳定性等待（过滤问题回显）
        let mut last = self.dom_reply_text(&q_norm);
        self.trace_html("ask_3_first_reply");
        // DOM 增量是否已流出：捕获晚到时不再回放 deltas，避免内容重复
        let mut streamed_any = false;

        let mut stable_since = Instant::now();
        loop {
            // 每次迭代优先检查网络层捕获
            let captured = self.reply_capture.lock().unwrap().clone();
            if let Some(cap) = captured {
                if !cap.reply.reply.is_empty() || !cap.reply.thinking.is_empty() {
                    let ctx = if streamed_any { None } else { chunk_tx.clone() };
                    return self.finish_with_capture(cap, ctx, "ask_4_reply_done").await;
                }
            }
            self.page.settle(1000).await;
            let cur = self.dom_reply_text(&q_norm);
            if cur != last {
                if let Some(tx) = &chunk_tx {
                    let delta = if cur.starts_with(&last) {
                        cur[last.len()..].to_string()
                    } else {
                        cur.clone()
                    };
                    let _ = tx.send(delta).await;
                    streamed_any = true;
                }
                last = cur;
                stable_since = Instant::now();
                continue;
            }
            let stopping = self.is_stopping();
            let has_content = !last.is_empty();
            if has_content && !stopping && stable_since.elapsed().as_millis() as u64 >= self.cfg.stable_ms {
                break;
            }
            if Instant::now() > deadline {
                break;
            }
        }
        // 稳定性 break 后最后再查一次网络层
        let captured = self.reply_capture.lock().unwrap().clone();
        if let Some(cap) = captured {
            if !cap.reply.reply.is_empty() || !cap.reply.thinking.is_empty() {
                let ctx = if streamed_any { None } else { chunk_tx.clone() };
                return self.finish_with_capture(cap, ctx, "ask_4_reply_done").await;
            }
        }
        if last.is_empty() {
            anyhow::bail!("等待回复超时（网络层与 DOM 均无有效内容），可能被风控或选择器失效");
        }
        self.trace_html("ask_4_reply_done");
        Ok(AiReply {
            thinking: String::new(),
            reply: last,
            ..Default::default()
        })
    }

    /// 网络层捕获完成：按 SSE 原始增量序列逐块回放给流式客户端。
    /// chunk_tx=None 时（DOM 兜底已流过内容）只返回结果不回放，避免重复；
    /// 同时 thinking 增量也走结构化 chunk 下发（此前整段裸文本丢失思考过程）。
    async fn finish_with_capture(
        &mut self,
        captured: CapturedReply,
        chunk_tx: Option<mpsc::Sender<String>>,
        trace_step: &str,
    ) -> anyhow::Result<AiReply> {
        if let Some(tx) = &chunk_tx {
            for (kind, text) in &captured.deltas {
                if !text.is_empty() {
                    let _ = tx.send(encode_chunk(*kind, text)).await;
                }
            }
        }
        self.trace_html(trace_step);
        Ok(captured.reply)
    }

    async fn do_ask(
        &mut self,
        question: String,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        if !self.is_logged_in() {
            anyhow::bail!(
                "{}尚未登录：请在「网页版账号」页对 DeepSeek 重新授权\
                 （或调用 POST /api/web-providers/deepseek/login/cdp 发起 CDP 登录）",
                crate::models::ERR_NOT_LOGGED_IN
            );
        }
        // 清除上一轮捕获与 completion 生命周期，重新注册 on_response 回调
        *self.reply_capture.lock().unwrap() = None;
        *self.completion_state.lock().unwrap() = CompletionState::None;
        self.register_response_callback();
        // 确保 Worker polyfill 在位（PoW worker 按需创建，此时注入仍来得及）
        self.inject_worker_polyfill();
        if let Err(e) = self.send_message(&question).await {
            // 发送失败且页面已不在登录态：凭证失效被前端登出——
            // 给出可操作的授权指引而非裸「找不到输入框」
            if !self.dom_logged_in() {
                anyhow::bail!(
                    "{}发送失败且页面已退出登录态：DeepSeek 登录凭证可能已失效，\
                     请在「网页版账号」页重新授权",
                    crate::models::ERR_NOT_LOGGED_IN
                );
            }
            return Err(e);
        }
        let reply = self.wait_reply(&question, chunk_tx).await?;
        Ok(reply)
    }
}

// ---- DeepSeek SSE 解析 ----

/// 片段类型是否为思考内容
fn is_thinking_fragment(ty: &str) -> bool {
    ty.contains("THINK") || ty.contains("REASON")
}

/// 解析 `.../fragments/<idx>/content` 中的片段下标；`-1` 表示最后一个片段
fn fragment_index(p: &str, len: usize) -> Option<usize> {
    let seg = p.split("fragments").nth(1)?;
    let idx = seg.trim_start_matches('/').split('/').next()?;
    if idx == "-1" {
        if len == 0 {
            None
        } else {
            Some(len - 1)
        }
    } else {
        idx.parse::<usize>().ok()
    }
}

/// 应用一条 JSON-Patch 操作到片段列表。
/// - BATCH：逐条递归子操作
/// - fragments 整对象：按新片段追加
/// - fragments/.../content：APPEND 追加 / SET 覆盖
/// - 无 p/o 的裸 `{"v":"..."}`：沿用上一条补丁的目标（DeepSeek 的增量压缩格式）
///
/// 同时把内容变更按到达顺序记入 deltas（流式回放用）；SET 覆盖非空旧值时
/// 置 content_replaced，由调用方重算 deltas 保证拼接一致性。
#[allow(clippy::too_many_arguments)]
fn apply_patch(
    p: &str,
    o: Option<&str>,
    v: &serde_json::Value,
    fragments: &mut Vec<(bool, String)>,
    last_target: &mut Option<usize>,
    deltas: &mut Vec<(ChunkKind, String)>,
    content_replaced: &mut bool,
) {
    // BATCH：子操作为相对路径，逐条递归
    if o == Some("BATCH") {
        if let Some(items) = v.as_array() {
            for item in items {
                let sp = item.get("p").and_then(|x| x.as_str()).unwrap_or("");
                let so = item.get("o").and_then(|x| x.as_str());
                let sv = item.get("v").cloned().unwrap_or(serde_json::Value::Null);
                apply_patch(sp, so, &sv, fragments, last_target, deltas, content_replaced);
            }
        }
        return;
    }
    // 裸续写：无 p/o，沿用上一条补丁目标
    if p.is_empty() && o.is_none() {
        if let (Some(i), Some(text)) = (*last_target, v.as_str()) {
            fragments[i].1.push_str(text);
            deltas.push((frag_kind(fragments[i].0), text.to_string()));
        }
        return;
    }
    // 片段列表操作（APPEND/INSERT 整片段，v 为对象或对象数组）。
    // 只认 p 就是 fragments 的路径：排除 fragments/-1/results（搜索结果数组）
    // 与 fragments/-1/queries 等子字段，它们不是片段对象。
    if p == "fragments" || p.ends_with("/fragments") {
        let items: Vec<&serde_json::Value> = match v.as_array() {
            Some(arr) => arr.iter().collect(),
            None => vec![v],
        };
        for item in items {
            let Some(f) = item.as_object() else { continue };
            let ty = f.get("type").and_then(|t| t.as_str()).unwrap_or("RESPONSE");
            let text = f.get("content").and_then(|c| c.as_str()).unwrap_or("");
            let is_think = is_thinking_fragment(ty);
            fragments.push((is_think, text.to_string()));
            *last_target = Some(fragments.len() - 1);
            if !text.is_empty() {
                deltas.push((frag_kind(is_think), text.to_string()));
            }
        }
        return;
    }
    // 片段内容补丁
    if p.contains("fragments") && p.ends_with("/content") {
        let Some(idx) = fragment_index(p, fragments.len()) else {
            return;
        };
        // 下标越界：按回复片段扩展（避免丢增量）
        while fragments.len() <= idx {
            fragments.push((false, String::new()));
        }
        *last_target = Some(idx);
        let text = v.as_str().unwrap_or("");
        match o {
            Some("SET") | Some("REPLACE") => {
                if !fragments[idx].1.is_empty() {
                    *content_replaced = true;
                } else if !text.is_empty() {
                    deltas.push((frag_kind(fragments[idx].0), text.to_string()));
                }
                fragments[idx].1 = text.to_string();
            }
            _ => {
                fragments[idx].1.push_str(text);
                deltas.push((frag_kind(fragments[idx].0), text.to_string()));
            }
        }
    }
    // 其他路径（response/status、token 用量等）忽略，last_target 保持不变
}

/// 片段思考标记 → chunk 事件类型
fn frag_kind(is_think: bool) -> ChunkKind {
    if is_think {
        ChunkKind::Thinking
    } else {
        ChunkKind::Text
    }
}

/// 从 SSE 流式响应体中提取 AI 回复（含原始增量序列，流式回放用）。
/// 新版 JSON-Patch 流协议为主，旧版直字段 / OpenAI choices 格式兜底。
fn extract_sse_reply(body: &str) -> Option<CapturedReply> {
    // ---------- 新版 JSON-Patch 流协议 ----------
    let mut fragments: Vec<(bool, String)> = Vec::new();
    let mut deltas: Vec<(ChunkKind, String)> = Vec::new();
    let mut content_replaced = false;
    let mut last_target: Option<usize> = None;
    let mut pending_event: Option<String> = None;
    let mut saw_new_format = false;

    for line in body.lines() {
        let line = line.trim();
        if let Some(ev) = line.strip_prefix("event:") {
            pending_event = Some(ev.trim().to_string());
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        // SSE 语义：event 头作用于下一条消息，分发后重置为默认 message
        let ev = pending_event.take().unwrap_or_default();
        if !ev.is_empty() && ev != "message" {
            continue; // ready / title / update_session / close 等元事件
        }
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };

        // 响应快照：{"v":{"response":{...,"fragments":[...]}}}
        if let Some(resp) = v.get("v").and_then(|x| x.get("response")) {
            saw_new_format = true;
            if let Some(frags) = resp.get("fragments").and_then(|f| f.as_array()) {
                fragments.clear();
                deltas.clear();
                for f in frags {
                    let ty = f.get("type").and_then(|t| t.as_str()).unwrap_or("RESPONSE");
                    let text = f.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    let is_think = is_thinking_fragment(ty);
                    fragments.push((is_think, text.to_string()));
                    if !text.is_empty() {
                        deltas.push((frag_kind(is_think), text.to_string()));
                    }
                }
                if !fragments.is_empty() {
                    last_target = Some(fragments.len() - 1);
                }
            }
            continue;
        }

        let p = v.get("p").and_then(|x| x.as_str()).unwrap_or("");
        let o = v.get("o").and_then(|x| x.as_str());
        if !p.is_empty() || o.is_some() {
            saw_new_format = true;
        }
        let val = v.get("v").cloned().unwrap_or(serde_json::Value::Null);
        apply_patch(p, o, &val, &mut fragments, &mut last_target, &mut deltas, &mut content_replaced);
    }

    if saw_new_format {
        let mut reply = String::new();
        let mut thinking = String::new();
        for (is_think, text) in &fragments {
            if text.is_empty() {
                continue;
            }
            if *is_think {
                thinking.push_str(text);
            } else {
                reply.push_str(text);
            }
        }
        let reply = reply.trim().to_string();
        let thinking = thinking.trim().to_string();
        if !reply.is_empty() || !thinking.is_empty() {
            // 片段 content 被整体覆盖：按最终片段内容重算增量（保拼接一致）
            if content_replaced {
                deltas.clear();
                for (is_think, text) in &fragments {
                    if !text.is_empty() {
                        deltas.push((frag_kind(*is_think), text.clone()));
                    }
                }
            }
            return Some(CapturedReply {
                reply: AiReply {
                    thinking,
                    reply,
                    ..Default::default()
                },
                deltas,
            });
        }
        return None;
    }

    // ---------- 旧版直字段 / OpenAI choices 格式 ----------
    let mut reply = String::new();
    let mut thinking = String::new();
    let mut deltas: Vec<(ChunkKind, String)> = Vec::new();

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

        // 直接字段格式：{"content":"...", "reasoning_content":"..."}
        if let Some(c) = v.get("content").and_then(|c| c.as_str()) {
            if !c.is_empty() {
                reply.push_str(c);
                deltas.push((ChunkKind::Text, c.to_string()));
            }
        }
        if let Some(rc) = v.get("reasoning_content").and_then(|c| c.as_str()) {
            if !rc.is_empty() {
                thinking.push_str(rc);
                deltas.push((ChunkKind::Thinking, rc.to_string()));
            }
        }

        // OpenAI choices 格式：{"choices":[{"delta":{"content":"..."}}]}
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                let delta = choice.get("delta").or_else(|| choice.get("message"));
                if let Some(d) = delta {
                    if let Some(c) = d.get("content").and_then(|c| c.as_str()) {
                        if !c.is_empty() {
                            reply.push_str(c);
                            deltas.push((ChunkKind::Text, c.to_string()));
                        }
                    }
                    if let Some(rc) = d.get("reasoning_content").and_then(|c| c.as_str()) {
                        if !rc.is_empty() {
                            thinking.push_str(rc);
                            deltas.push((ChunkKind::Thinking, rc.to_string()));
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
        Some(CapturedReply {
            reply: AiReply {
                thinking,
                reply,
                ..Default::default()
            },
            deltas,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按真实落盘（html/deepseek_*_sse.txt）的协议形态构造：
    /// 快照(SEARCH) → results 数组 → BATCH(SEARCH 状态) → BATCH(APPEND RESPONSE 片段)
    /// → 显式 content APPEND → 裸 v 续写 → status SET FINISHED → event: close
    #[test]
    fn json_patch_protocol_with_deltas() {
        let body = concat!(
            "event: ready\n",
            "data: {\"request_message_id\":5}\n\n",
            "data: {\"v\":{\"response\":{\"status\":\"WIP\",\"fragments\":[{\"id\":2,\"type\":\"SEARCH\",\"status\":\"WIP\",\"content\":null,\"queries\":[{\"query\":\"q1\"}],\"results\":[]}]}}}\n\n",
            "data: {\"p\":\"response/fragments/-1/results\",\"v\":[{\"url\":\"https://e.com/1\",\"title\":\"来源1\",\"snippet\":\"s\",\"cite_index\":1}]}\n\n",
            "data: {\"p\":\"response/fragments/-1\",\"o\":\"BATCH\",\"v\":[{\"p\":\"status\",\"v\":\"FINISHED\"},{\"p\":\"content\",\"v\":\"Found 1 web page\"}]}\n\n",
            "data: {\"p\":\"response\",\"o\":\"BATCH\",\"v\":[{\"p\":\"fragments\",\"o\":\"APPEND\",\"v\":[{\"id\":3,\"type\":\"RESPONSE\",\"content\":\"成都\",\"references\":[]}]},{\"p\":\"has_pending_fragment\",\"o\":\"SET\",\"v\":false}]}\n\n",
            "data: {\"p\":\"response/fragments/-1/content\",\"o\":\"APPEND\",\"v\":\"本地\"}\n\n",
            "data: {\"v\":\"提供\"}\n\n",
            "data: {\"v\":\"GEO 服务\"}\n\n",
            "data: {\"p\":\"response/status\",\"o\":\"SET\",\"v\":\"FINISHED\"}\n\n",
            "event: close\n",
            "data: {\"click_behavior\":\"none\"}\n",
        );
        let cap = extract_sse_reply(body).unwrap();
        assert_eq!(cap.reply.reply, "成都本地提供GEO 服务");
        assert!(cap.reply.thinking.is_empty());
        // SEARCH 片段的 "Found 1 web page" 不得混入正文（相对路径 BATCH 子操作被忽略）
        assert!(!cap.reply.reply.contains("Found"));
        // deltas 拼接必须与 reply 一致（trim 差异除外）
        let joined: String = cap
            .deltas
            .iter()
            .filter(|(k, _)| matches!(k, ChunkKind::Text))
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(joined, cap.reply.reply);
        // 增量块数：快照(SEARCH 无 content) + BATCH APPEND(成都) + 显式 APPEND(本地) + 2 条裸 v
        assert_eq!(cap.deltas.len(), 4);
        assert!(cap.deltas.iter().all(|(k, _)| matches!(k, ChunkKind::Text)));
    }

    /// THINKING 片段增量走 ChunkKind::Thinking
    #[test]
    fn thinking_fragment_deltas() {
        let body = concat!(
            "data: {\"v\":{\"response\":{\"fragments\":[{\"type\":\"THINKING\",\"content\":\"先想\"}]}}}\n\n",
            "data: {\"v\":\"再想\"}\n\n",
            "data: {\"p\":\"response\",\"o\":\"BATCH\",\"v\":[{\"p\":\"fragments\",\"o\":\"APPEND\",\"v\":[{\"type\":\"RESPONSE\",\"content\":\"答案\"}]}]}\n\n",
            "data: {\"v\":\"也续\"}\n\n",
            "data: {\"p\":\"response/status\",\"o\":\"SET\",\"v\":\"FINISHED\"}\n",
        );
        let cap = extract_sse_reply(body).unwrap();
        assert_eq!(cap.reply.thinking, "先想再想");
        assert_eq!(cap.reply.reply, "答案也续");
        let think: String = cap
            .deltas
            .iter()
            .filter(|(k, _)| matches!(k, ChunkKind::Thinking))
            .map(|(_, t)| t.as_str())
            .collect();
        let text: String = cap
            .deltas
            .iter()
            .filter(|(k, _)| matches!(k, ChunkKind::Text))
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(think, cap.reply.thinking);
        assert_eq!(text, cap.reply.reply);
    }

    /// content SET 覆盖非空旧值：deltas 重算为最终片段内容
    #[test]
    fn content_set_replaces_recomputes_deltas() {
        let body = concat!(
            "data: {\"v\":{\"response\":{\"fragments\":[{\"type\":\"RESPONSE\",\"content\":\"旧内容前缀\"}]}}}\n\n",
            "data: {\"v\":\"旧增量\"}\n\n",
            "data: {\"p\":\"response/fragments/-1/content\",\"o\":\"SET\",\"v\":\"新内容\"}\n\n",
            "data: {\"v\":\"+新增量\"}\n\n",
            "data: {\"p\":\"response/status\",\"o\":\"SET\",\"v\":\"FINISHED\"}\n",
        );
        let cap = extract_sse_reply(body).unwrap();
        assert_eq!(cap.reply.reply, "新内容+新增量");
        let joined: String = cap
            .deltas
            .iter()
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(joined, cap.reply.reply);
    }

    /// 真实落盘（html/deepseek_*_sse.txt）端到端解析；文件不存在时跳过。
    /// 落盘样例：搜索对话，724 条裸 v 增量 + BATCH 片段操作 + 元事件。
    #[test]
    fn real_captured_sse_file() {
        let Ok(body) = std::fs::read_to_string("html/deepseek_1792489292961_sse.txt") else {
            return;
        };
        let cap = extract_sse_reply(&body).expect("真实 SSE 应解析出回复");
        assert!(!cap.reply.reply.is_empty());
        assert!(cap.reply.reply.contains("生成式引擎优化"));
        assert!(cap.reply.reply.contains("成都"), "BATCH APPEND 首词不得丢失: {}", &cap.reply.reply[..30.min(cap.reply.reply.len())]);
        // deltas 拼接与最终 reply 一致（trim 差异除外）
        let joined: String = cap
            .deltas
            .iter()
            .filter(|(k, _)| matches!(k, ChunkKind::Text))
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(joined.trim(), cap.reply.reply);
        assert!(cap.deltas.len() > 100, "应捕获数百条增量，实际 {}", cap.deltas.len());
    }

    /// 旧版直字段格式也产 deltas
    #[test]
    fn legacy_direct_field_deltas() {
        let body = concat!(
            "data: {\"content\":\"你好\",\"reasoning_content\":\"思考1\"}\n\n",
            "data: {\"content\":\"，世界\"}\n\n",
            "data: [DONE]\n",
        );
        let cap = extract_sse_reply(body).unwrap();
        assert_eq!(cap.reply.reply, "你好，世界");
        assert_eq!(cap.reply.thinking, "思考1");
        assert_eq!(cap.deltas.len(), 3);
        let think: String = cap
            .deltas
            .iter()
            .filter(|(k, _)| matches!(k, ChunkKind::Thinking))
            .map(|(_, t)| t.as_str())
            .collect();
        assert_eq!(think, "思考1");
    }
}