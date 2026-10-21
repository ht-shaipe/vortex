//! 豆包平台 provider：扫码/短信登录、ask（思考过程/搜索关键字/参考资料分离）、
//! cookie 持久化免登录、网络层 AI 回复捕获（SSE ground truth）。

use std::sync::Arc;
use std::time::{Duration, Instant};

use obscura::{Browser, InterceptResolution};
use tokio::sync::mpsc;

use crate::common::chunk::{encode_chunk, ChunkKind};
use crate::common::cookie_import::parse_cookie_import;
use crate::common::extract::{block_text_value, json_find_text, merge_ai, parse_sse_events};
use crate::common::fill::build_fill_js;
use crate::common::qr::save_qr_image;
use crate::common::util::{dump_page_html, trace_page_html};
use crate::config::ProviderCfg;
use crate::models::*;
use crate::providers::Provider;

/// 豆包 provider 入口（Provider trait 实现）
pub struct DoubaoProvider;

impl Provider for DoubaoProvider {
    const NAME: &'static str = "doubao";
    const DISPLAY: &'static str = "豆包";
    const DEFAULT_URL: &'static str = "https://www.doubao.com/chat/";
    const URL_ENV: &'static str = "DOUBAO_URL";

    async fn run(cfg: Arc<ProviderCfg>, rx: mpsc::Receiver<Command>) {
        let browser = match Browser::builder()
            .stealth(true)
            .storage_dir(&cfg.storage_dir)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("[doubao] obscura 启动失败: {e}");
                return;
            }
        };
        let page = match browser.new_page().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[doubao] 新建页面失败: {e}");
                return;
            }
        };
        Doubao::new(browser, page, cfg).run(rx).await;
    }
}

/// 持有 obscura 的 Browser 与唯一一个 Page（豆包标签页）。
/// 所有对豆包的导航 / 输入 / 读取都在这个任务内串行执行。
pub struct Doubao {
    browser: Browser,
    page: obscura::Page,
    cfg: Arc<ProviderCfg>,
    /// 调试模式运行时开关（初始值来自 cfg，可通过 API 切换）
    debug_html: bool,
    /// 最近一次 check_qrconnect 轮询的扫码状态与时间（网络层捕获，ground truth）。
    /// 页面登录弹窗每 1s 轮询一次扫码状态，on_response 回调把 status 存到这里。
    qr_poll: Arc<std::sync::Mutex<Option<(String, Instant)>>>,
    /// 自愈路径重建过 page，run() 主循环需要重新注册网络拦截/响应捕获
    page_recreated: bool,
    /// 页面重建后是否自动导航回站点（reset 换新页后需要，自愈路径自己会导航）
    navigate_after_recreate: bool,
    /// 登录弹窗最新一次 get_qrcode 的 token（on_response 捕获）。
    /// 豆包二维码 60s 过期后页面自动换新 token，据此检测刷新并重存二维码文件
    qr_token: Arc<std::sync::Mutex<Option<String>>>,
    /// 上次保存二维码文件时的 token；与 qr_token 不一致说明页面已刷新二维码
    saved_qr_token: Option<String>,
    /// 本轮 token 刷新的重存尝试次数（DOM 渲染慢时重试，超限放弃等下次刷新）
    qr_refresh_retries: u32,
    /// 心跳计数，用于 cookie 周期落盘节流
    cookie_tick: u32,
    /// 网络层捕获的 AI 回复（on_response 从 SSE/streaming 响应中提取，含思考过程）。
    /// DOM 渲染可能因 CDN JS 加载失败而缺失，网络层是 ground truth。
    reply_capture: Arc<std::sync::Mutex<Option<(AiReply, Instant)>>>,
    /// 本轮 ask 的 /chat/completion 请求生命周期：
    /// None（未发起）→ InFlight（请求已发出）→ Done（响应已完整到达）。
    /// 用于 wait_reply 判断网络路径是否启用：completion 在途时 DOM 的
    /// "稳定文本"（如刚发出的问题回显）不可作为回复依据，必须等网络层。
    completion_state: Arc<std::sync::Mutex<CompletionState>>,
    /// on_response 回调 ID，用于在重新注册前移除旧回调
    response_cb_id: Option<u64>,
}

/// /chat/completion 请求生命周期
#[derive(Clone, Copy, PartialEq)]
enum CompletionState {
    /// 尚未发起（本轮 ask 重置后的初始态）
    None,
    /// 请求已发出（拦截层看到），响应未回
    InFlight,
    /// 响应已完整到达（on_response 看到，无论提取成功与否）
    Done,
}

// ---- 注入页面的 JS 片段（与 Node 版逻辑一致，仅改为 obscura 进程内 evaluate）----

/// 取最后一条助手消息文本。
/// 优先用 data-testid 精确匹配 AI 回复（data-reply-message="true" 容器内的内容），
/// 排除已发送消息（data-send-message-boundary）和输入区域（chat_input / file_drop_area）。
const READ_LAST_JS: &str = r#"(() => {
  const isInputArea = (el) => {
    if (!el) return false;
    const tid = el.getAttribute('data-testid') || '';
    if (/input|file_drop|upload|drag/i.test(tid)) return true;
    return !!el.closest('[data-testid*="chat_input"], [data-testid*="file_drop_area"]');
  };
  const isSentMessage = (el) => {
    return !!el.closest('[data-send-message-boundary]');
  };
  // 排除「推荐追问」chip（suggest_message_item）：它是建议的下一个问题，
  // 不是 AI 回复，误当回复会把追问问题当成答案返回
  const isSuggest = (el) => {
    return !!el.closest('[data-testid="suggest_message_item"], [data-testid*="suggest_message"]');
  };
  // 排除 UI 噪声文本（文件上传提示、导航菜单等）
  const isNoiseText = (t) => {
    if (t.length < 3) return true;
    return /拖放|文件数量|文件类型|新工作|新对话|定时任务|插件|技能|伙伴|云盘|服务|更多项目/.test(t);
  };

  // 1. 优先：data-reply-message="true" 容器内的 message_text_content
  const replyContainers = [...document.querySelectorAll('[data-reply-message="true"]')];
  for (const rc of replyContainers) {
    const textEls = [...rc.querySelectorAll('[data-testid="message_text_content"], [data-testid="message_content"]')];
    const texts = textEls.map(el => (el.textContent || '').trim()).filter(t => t.length > 2 && !isNoiseText(t));
    if (texts.length) return texts[texts.length - 1];
    const rcText = (rc.textContent || '').trim();
    if (rcText.length > 2 && !isNoiseText(rcText)) return rcText;
  }

  // 2. message_text_content / message_content，排除已发送和输入区域
  const msgEls = [...document.querySelectorAll('[data-testid="message_text_content"], [data-testid="message_content"]')]
    .filter(el => !isInputArea(el) && !isSentMessage(el) && !isSuggest(el) && el.offsetParent !== null);
  if (msgEls.length) {
    const texts = msgEls.map(el => (el.textContent || '').trim()).filter(t => t.length > 2 && !isNoiseText(t));
    if (texts.length) return texts[texts.length - 1];
  }

  // 3. class 模式匹配，排除已发送和输入/上传区域
  const sels = [
    '[class*="receive"]', '[class*="assistant"]', '[class*="bot"]',
    '[class*="message"]', '[class*="answer"]', '[class*="reply"]',
    '[class*="bubble"]', '[class*="response"]', '[class*="markdown"]',
    '[class*="msg"]',
    '[data-testid*="answer"]'
  ];
  const nodes = [...document.querySelectorAll(sels.join(','))]
    .filter(el => !isInputArea(el) && !isSentMessage(el) && !isSuggest(el))
    .map(el => (el.textContent || '').trim())
    .filter(t => t.length > 2 && !isNoiseText(t));
  if (nodes.length) return nodes[nodes.length - 1];

  // 4. 兜底：最深的非空 div（排除输入区域、已发送、UI 噪声）
  const divs = [...document.querySelectorAll('div')]
    .filter(d => d.children.length === 0 && !isInputArea(d) && !isSentMessage(d) && !isSuggest(d))
    .map(d => (d.textContent || '').trim())
    .filter(t => t.length > 10 && !isNoiseText(t));
  return divs.length ? divs[divs.length - 1] : '';
})()"#;

/// 是否还在生成（"停止生成"按钮可见）
const STOPPING_JS: &str = r#"(() => {
  const b = [...document.querySelectorAll('button')].find(b => /停止生成/.test(b.textContent || ''));
  return !!b;
})()"#;

/// 发送按钮就绪检测：豆包只在编辑器状态非空时渲染 chat_input_send_button
/// （空输入渲染的是 chat_input_local_break_button 换行按钮），且按钮可用。
/// 按钮出现即可确认填入的文本已同步进编辑器内部状态。
const SEND_BTN_READY_JS: &str = r#"(() => {
  const b = document.querySelector('[data-testid="chat_input_send_button"]');
  return !!b && !b.disabled;
})()"#;

/// 清空豆包编辑器（ProseMirror view → tiptap commands → DOM 兜底）。
/// 重填前清场用：fill 的 ok-dom 兜底策略只改 DOM 不同步编辑器状态，
/// 状态与 DOM 残留不一致时先清空再重填，避免文本叠加或残留干扰。
const CLEAR_EDITOR_JS: &str = r#"(() => {
  const el = document.querySelector('.ProseMirror[contenteditable="true"]');
  if (!el) return 'no-input';
  const find = (pred) => {
    const seen = new Set();
    const queue = [el, el.parentElement, document.body];
    while (queue.length) {
      const n = queue.shift();
      if (!n || seen.has(n) || seen.size > 2000) continue;
      seen.add(n);
      for (const k of Object.keys(n)) {
        try {
          const v = n[k];
          if (v && typeof v === 'object') {
            if (pred(v)) return v;
            queue.push(v);
          }
        } catch (e) {}
      }
      for (const k of Object.getOwnPropertyNames(n)) {
        if (k.startsWith('__reactFiber') || k.startsWith('__reactInternalInstance')) {
          try {
            const f = n[k];
            if (f) {
              queue.push(f);
              if (f.memoizedProps) queue.push(f.memoizedProps);
              if (f.memoizedState) queue.push(f.memoizedState);
            }
          } catch (e) {}
        }
      }
    }
    return null;
  };
  try {
    const view = find((v) => v.view && v.state && typeof v.dispatch === 'function');
    if (view && view.state) {
      view.dispatch(view.state.tr.delete(0, view.state.doc.content.size));
      return 'ok-pm';
    }
  } catch (e) {}
  try {
    let editor = find((v) => (v.commands && typeof v.commands.insertContent === 'function')
      || (v.editor && v.editor.commands));
    if (editor && editor.editor && editor.editor.commands) editor = editor.editor;
    if (editor && editor.commands) {
      if (typeof editor.commands.clearContent === 'function') editor.commands.clearContent();
      else if (typeof editor.commands.selectAll === 'function'
        && typeof editor.commands.deleteSelection === 'function') {
        editor.commands.selectAll();
        editor.commands.deleteSelection();
      } else return 'not-cleared';
      return 'ok-tiptap';
    }
  } catch (e) {}
  try {
    el.innerHTML = '';
    el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'deleteContentBackward' }));
    return 'ok-dom';
  } catch (e) {}
  return 'not-cleared';
})()"#;

/// 登录态：有可见"登录"按钮则未登录；否则看是否有可见输入框。
/// 注意：不能仅凭 textContent 包含"登录"判断——豆包对话标题/任务名可能含"登录"
/// 二字（如"小程序登录背景图生成"），会误判为登录按钮。
/// 策略：优先用 data-testid 精确匹配；回退到文本匹配时要求按钮短文本就是"登录"
/// 或"登录/注册"等（trim 后 ≤6 字），排除长文本的任务标题。
const LOGGED_IN_JS: &str = r#"(() => {
  // 0. URL 在登录/SSO 路由或 passport 域 → 未登录（最高优先级）。
  // token 失效被前端登出时可能整页跳登录路由，登录页自带输入框会
  // 命中下方条件 3 误判已登录（deepseek 实测同款坑）
  if (/\/(login|log-in|signin|sign-in|sso|passport)([/?#]|$)/i.test(location.pathname)
      || /(passport|login)\./i.test(location.host)) return false;
  // 1. 精确 data-testid 匹配（最可靠）
  if (document.querySelector('[data-testid="to_login_button"]')) return false;
  // 2. 文本匹配：要求 trim 后文本以"登录"开头且很短（真正的登录按钮）
  const loginBtn = [...document.querySelectorAll('button, a, [role="button"]')]
    .find(el => {
      if (el.offsetParent === null) return false;
      const t = (el.textContent || '').trim();
      if (t.length === 0 || t.length > 8) return false;
      return /^登录|^登\s*录|^log\s*in/i.test(t);
    });
  if (loginBtn) return false;
  // 3. 有可见输入框则认为已登录
  const els = [...document.querySelectorAll('[contenteditable="true"], textarea')];
  return els.some(e => (e.offsetParent !== null) || e.getClientRects().length > 0);
})()"#;

/// 读取登录弹窗二维码状态。豆包扫码轮询状态机（逆向自 component-login-modal）：
/// new（等待扫码）→ scanned（弹窗显示「扫码成功/请在手机上确认」）
/// → confirmed（自动登录）| refused | expired（显示「二维码失效/点击刷新」）。
/// 返回 'scanned' / 'expired' / 'ready'（弹窗开着、码在等扫）/ 'none'。
const QR_STATE_JS: &str = r#"(() => {
  const vis = (el) => !!(el && (el.offsetParent !== null || el.getClientRects().length > 0));
  const textVis = (t) => [...document.querySelectorAll('div,span,p')]
    .some(el => (el.textContent || '').trim() === t && vis(el));
  if (textVis('扫码成功') && textVis('请在手机上确认')) return 'scanned';
  if (textVis('二维码失效') && textVis('点击刷新')) return 'expired';
  // 注意：qrcode_switcher（空 div）在文档序上先于 qrcode_image，必须遍历所有匹配
  const hosts = [...document.querySelectorAll(
    '[data-testid*="qrcode"],[class*="qrcode"],[class*="QRCode"],[class*="Qrcode"],[class*="QRcode"]')];
  if (hosts.some(h => vis(h) && h.querySelector('svg,canvas,img'))) return 'ready';
  return 'none';
})()"#;

/// 抽取登录二维码（在单个 document 内执行）：
/// 1) 二维码容器（data-testid/class 含 qrcode）内的 SVG —— 豆包新版把二维码
///    渲染为内联 SVG（数百个 circle/rect），XMLSerializer 序列化成 data URL 即可被 <img> 显示；
/// 2) 近正方形且元素密集的独立 SVG（二维码特征：正方形 + 大量子节点）；
/// 3) canvas：登录区优先 → 近正方形 → 任意（空白 canvas 的 dataURL <500 字符，据此过滤）；
/// 4) img：src 含 qr/scan/passport、登录区内、近正方形大图、首个大图兜底。
const EXTRACT_QR_JS: &str = r#"(() => {
  const svgUrl = (svg) => 'data:image/svg+xml;charset=utf-8,'
    + encodeURIComponent(new XMLSerializer().serializeToString(svg));
  const grab = (doc) => {
    const near = (a, b) => Math.abs(a - b) <= Math.max(a, b) * 0.3;
    const inLogin = (el) => !!el.closest(
      '[class*="login"],[class*="Login"],[class*="qr"],[class*="QR"],[class*="Qr"],[class*="passport"],[id*="login"],[id*="qr"]');
    // 1) 二维码容器内的 svg / canvas
    const host = doc.querySelector(
      '[data-testid*="qrcode"],[class*="qrcode"],[class*="QRCode"],[class*="Qrcode"],[class*="QRcode"]');
    if (host) {
      const hsvg = host.querySelector('svg');
      if (hsvg && hsvg.querySelectorAll('*').length > 10) { try { return svgUrl(hsvg); } catch (e) {} }
      const hcv = host.querySelector('canvas');
      if (hcv) { try { const u = hcv.toDataURL('image/png'); if (u.length > 500) return u; } catch (e) {} }
    }
    // 2) 近正方形、元素密集的独立 SVG
    const dsvg = [...doc.querySelectorAll('svg')].find(s => {
      const w = parseFloat(s.getAttribute('width') || '0');
      const h = parseFloat(s.getAttribute('height') || '0');
      return w >= 80 && h >= 80 && near(w, h) && s.querySelectorAll('*').length > 50;
    });
    if (dsvg) { try { return svgUrl(dsvg); } catch (e) {} }
    // 3) canvas
    const cvs = [...doc.querySelectorAll('canvas')].filter(c => c.width >= 80 && c.height >= 80);
    const cv = cvs.find(inLogin) || cvs.find(c => near(c.width, c.height)) || cvs[0];
    if (cv) { try { const u = cv.toDataURL('image/png'); if (u.length > 500) return u; } catch (e) {} }
    // 4) img 兜底
    const imgs = [...doc.querySelectorAll('img')]
      .filter(i => i.width >= 80 && i.height >= 80 && (i.currentSrc || i.src));
    const pick = imgs.find(i => /qr|qrcode|scan|passport|login/i.test(i.currentSrc || i.src) || inLogin(i))
      || imgs.find(i => near(i.width, i.height))
      || imgs[0];
    return pick ? (pick.currentSrc || pick.src) : '';
  };
  let r = '';
  try { r = grab(document); } catch (e) {}
  if (r) return r;
  for (const f of document.querySelectorAll('iframe')) {
    try { if (f.contentDocument) { r = grab(f.contentDocument); if (r) return r; } } catch (e) {}
  }
  return '';
})()"#;

/// 点击右上角登录按钮（触发二维码弹窗）。
/// 优先点 data-testid；offsetParent 对 fixed 头部元素为 null，会漏判，需放宽重试。
/// 点击前补发 pointer/mouse 事件序列（部分组件监听 pointerdown 而非 click）。
/// 返回 'CLICKED:描述' / 'NONE'，便于日志定位实际点到了谁。
const CLICK_LOGIN_JS: &str = r#"(() => {
  let target = document.querySelector(
    '[data-testid="to_login_button"], [data-testid="login_button"], [data-testid="login-button"]');
  if (!target || target.offsetParent === null) {
    const els = [...document.querySelectorAll('button, a, [role="button"], [class*="login"], [class*="Login"]')]
      .filter(el => /登录|登\s*录|log\s*in/i.test(el.textContent || '') && (el.offsetParent !== null));
    els.sort((a, b) => (a.textContent || '').trim().length - (b.textContent || '').trim().length);
    target = els[0] || null;
  }
  if (!target || target.offsetParent === null) {
    // fixed 头部按钮 offsetParent 为 null，上面的可见性过滤会漏掉——直接兜底
    target = document.querySelector('[data-testid="to_login_button"]');
  }
  if (!target) return 'NONE';
  const desc = target.tagName + '#'
    + (target.getAttribute('data-testid') || '')
    + '.' + String(target.className || '').slice(0, 60);
  for (const t of ['pointerdown', 'mousedown', 'pointerup', 'mouseup']) {
    try {
      target.dispatchEvent(new MouseEvent(t, {bubbles: true, cancelable: true, view: window}));
    } catch (e) {}
  }
  target.click();
  return 'CLICKED:' + desc;
})()"#;

/// 登录弹窗是否已打开：只认登录特征（二维码容器 / 登录方式文案）。
/// 注意不能泛匹配 role="dialog" 或 class 含 modal——豆包首页会自动弹
/// 「下载豆包电脑版」推广弹窗，同样是 role="dialog"，泛匹配会误判成登录弹窗。
const DIALOG_OPEN_JS: &str = r#"(() => {
  const host = document.querySelector(
    '[data-testid*="qrcode"],[class*="qrcode"],[class*="QRCode"],[class*="Qrcode"],[class*="QRcode"]');
  if (host && host.offsetParent !== null) return true;
  return [...document.querySelectorAll('a, button, span, div')]
    .filter(el => el.offsetParent !== null && (el.textContent || '').trim().length <= 12)
    .some(el => /手机号登录|扫码登录|二维码登录|飞书账号|抖音一键|验证码登录/.test(el.textContent || ''));
})()"#;

/// 关掉挡路的弹窗（如自动弹出的「下载豆包电脑版」推广弹窗）。
/// 只关不含登录特征的可见弹窗：点其内部的关闭按钮（aria-label=关闭 / closeButton 类）。
/// 返回 true 表示关了一个弹窗（调用方应等待其退出动画）。
const CLOSE_PROMO_JS: &str = r#"(() => {
  const loginMarker = (root) =>
    /手机号登录|扫码登录|二维码登录|验证码登录|qrcode/i.test(root.textContent || '');
  const dialogs = [...document.querySelectorAll('[role="dialog"], .semi-modal')]
    .filter(el => (el.offsetParent !== null) && !loginMarker(el));
  for (const dlg of dialogs) {
    const btn = dlg.querySelector(
      'button[aria-label="关闭"], button[aria-label="Close" i], button[aria-label="close" i], [class*="closeButton"], [class*="close-btn"], [data-slot="dialog-close"]');
    if (btn) { btn.click(); return true; }
  }
  return false;
})()"#;

/// 切换到「手机号登录」标签页（在登录弹窗内寻找含"手机号"文本的可点击元素并点击）
const SWITCH_PHONE_TAB_JS: &str = r#"(() => {
  const els = [...document.querySelectorAll('a, button, span, div, [role="tab"], [role="button"]')]
    .filter(el => /手机号|手机/.test(el.textContent || '')
      && (el.offsetParent !== null)
      && (el.textContent || '').trim().length <= 10);
  if (!els.length) return false;
  els.sort((a, b) => (a.textContent || '').trim().length - (b.textContent || '').trim().length);
  els[0].click();
  return true;
})()"#;

/// 在手机号输入框中填入手机号（参数为 JSON 编码的手机号字符串）
const FILL_PHONE_JS: &str = r#"((phone) => {
  const inputs = [...document.querySelectorAll('input')]
    .filter(i => i.offsetParent !== null);
  const el = inputs.find(i => i.type === 'tel')
    || inputs.find(i => /手机|phone|mobile/i.test(i.placeholder || ''))
    || inputs.find(i => /手机|phone|mobile/i.test(i.name || ''))
    || inputs.find(i => /1[3-9]\d{9}/.test(i.placeholder || ''));
  if (!el) return false;
  el.focus();
  el.value = phone;
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return true;
})"#;

/// 点击「发送验证码」/「获取验证码」按钮
const SEND_CODE_JS: &str = r#"(() => {
  const btns = [...document.querySelectorAll('button, a, [role="button"], span')]
    .filter(b => b.offsetParent !== null);
  const btn = btns.find(b => /发送验证码|获取验证码|发送|获取/.test(b.textContent || '')
    && (b.textContent || '').trim().length <= 10);
  if (btn) { btn.click(); return true; }
  return false;
})()"#;

/// 在验证码输入框中填入验证码（参数为 JSON 编码的验证码字符串）
const FILL_CODE_JS: &str = r#"((code) => {
  const inputs = [...document.querySelectorAll('input')]
    .filter(i => i.offsetParent !== null);
  const el = inputs.find(i => /验证码|code|verify|captcha/i.test(i.placeholder || ''))
    || inputs.find(i => /验证码|code|verify|captcha/i.test(i.name || ''))
    || inputs.find(i => /code|verify/i.test(i.id || ''));
  if (!el) return false;
  el.focus();
  el.value = code;
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return true;
})"#;

/// 点击登录弹窗内的「登录」/「确认」按钮（提交验证码后完成登录）
const CLICK_PHONE_LOGIN_BTN_JS: &str = r#"(() => {
  const btns = [...document.querySelectorAll('button, [role="button"]')]
    .filter(b => b.offsetParent !== null && !b.disabled);
  const btn = btns.find(b => /^登录$|^确认$|^确定$|^登\s*录$/.test((b.textContent || '').trim()))
    || btns.find(b => /登录|确认|确定/.test(b.textContent || '')
      && (b.textContent || '').trim().length <= 6);
  if (btn) { btn.click(); return true; }
  return false;
})()"#;

/// SVG→PNG 规范重建（obscura 无 Image 加载管线，浏览器原生光栅化不可用）。
/// 不复刻圆点样式，而是把 qr-code-styling 的 SVG 还原成**标准方点 QR**：
///   - 数据点：dot-color clipPath 里的 circle/rect → 最近模块网格 → 实心方块
///   - 定位角：corners-square → 7x7 外环；corners-dot → 3x3 中心
///   - 四周补 4 模块静区，模块 8px，任何扫码器（含 cv2）都能稳定识别
/// 结构识别不全时返回 DIAG: 诊断串，调用方退回 SVG data URL。
const RENDER_QR_PNG_JS: &str = r#"(() => {
  try {
    const host = document.querySelector(
      '[data-testid*="qrcode"],[class*="qrcode"],[class*="QRCode"],[class*="Qrcode"],[class*="QRcode"]');
    let svg = host ? host.querySelector('svg') : null;
    if (!svg) {
      svg = [...document.querySelectorAll('svg')].find(s => {
        const w = parseFloat(s.getAttribute('width') || '0');
        const h = parseFloat(s.getAttribute('height') || '0');
        return w >= 80 && h >= 80 && s.querySelectorAll('*').length > 50;
      }) || null;
    }
    if (!svg) return 'DIAG:NOSVG';

    const w = parseFloat(svg.getAttribute('width') || '200');
    const h = parseFloat(svg.getAttribute('height') || '200');
    // 只有「可见 rect」带 clip-path 属性（clipPath 内部形状不带），文档序：
    // background → dot → 三组 corner（square、dot）
    const rects = [...svg.querySelectorAll('rect')].filter(el => el.getAttribute('clip-path'));
    if (!rects.length) return 'DIAG:NORECT';

    let bgColor = '#ffffff';
    let dotFill = '#000000';
    let dotClip = null;
    const squares = [];
    const cDots = [];
    for (const r of rects) {
      const m = (r.getAttribute('clip-path') || '').match(/url\(['"]?#([^)'"]+)/);
      if (!m) continue;
      const id = m[1];
      const x = parseFloat(r.getAttribute('x') || '0');
      const y = parseFloat(r.getAttribute('y') || '0');
      const rw = parseFloat(r.getAttribute('width') || '0');
      const rh = parseFloat(r.getAttribute('height') || '0');
      const fill = r.getAttribute('fill') || '#000000';
      if (/background/.test(id)) {
        bgColor = fill;
      } else if (/corners-square/.test(id)) {
        squares.push([x, y, rw, rh, fill]);
      } else if (/corners-dot/.test(id)) {
        cDots.push([x, y, rw, rh, fill]);
      } else if (/dot-color|dots-color|module/.test(id)) {
        dotClip = document.getElementById(id);
        dotFill = fill;
      }
    }
    if (!dotClip || squares.length < 3 || cDots.length < 3) {
      return 'DIAG:STRUCT:' + squares.length + ',' + cDots.length + ',' + (dotClip ? 1 : 0);
    }

    // 模块大小 = 定位角边长 / 7；网格 N = ceil(w/mod)
    const mod = squares[0][2] / 7;
    if (!(mod > 0)) return 'DIAG:MOD';
    const N = Math.round(w / mod);
    if (N < 21 || N > 177) return 'DIAG:N:' + N;

    const S = 4;  // 每模块像素（4px 足够所有扫码器，控制响应体积）
    const Q = 4;  // 静区模块数
    const canvas = document.createElement('canvas');
    canvas.width = (N + 2 * Q) * S;
    canvas.height = (N + 2 * Q) * S;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = bgColor;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    const cell = (gx, gy, fill) => {
      if (gx < 0 || gx >= N || gy < 0 || gy >= N) return;
      ctx.fillStyle = fill;
      ctx.fillRect((gx + Q) * S, (gy + Q) * S, S, S);
    };

    // 数据点 → 最近模块网格
    const cells = [];
    for (const c of dotClip.querySelectorAll('circle')) {
      const cx = parseFloat(c.getAttribute('cx') || '0');
      const cy = parseFloat(c.getAttribute('cy') || '0');
      cells.push([Math.round(cx / mod - 0.5), Math.round(cy / mod - 0.5)]);
    }
    for (const rc of dotClip.querySelectorAll('rect')) {
      const rx = parseFloat(rc.getAttribute('x') || '0');
      const ry = parseFloat(rc.getAttribute('y') || '0');
      const rrw = parseFloat(rc.getAttribute('width') || '0');
      const rrh = parseFloat(rc.getAttribute('height') || '0');
      cells.push([
        Math.round((rx + rrw / 2) / mod - 0.5),
        Math.round((ry + rrh / 2) / mod - 0.5),
      ]);
    }
    for (const g of cells) {
      cell(g[0], g[1], dotFill);
    }

    // 定位角：7x7 外环（边框 1 模块）
    for (const s of squares) {
      const gx0 = Math.round(s[0] / mod);
      const gy0 = Math.round(s[1] / mod);
      for (let i = 0; i < 7; i++) {
        for (let j = 0; j < 7; j++) {
          if (i === 0 || i === 6 || j === 0 || j === 6) {
            cell(gx0 + i, gy0 + j, s[4]);
          }
        }
      }
    }
    // 定位角中心：3x3
    for (const d of cDots) {
      const gx0 = Math.round(d[0] / mod);
      const gy0 = Math.round(d[1] / mod);
      const n = Math.max(1, Math.round(d[2] / mod));
      for (let i = 0; i < n; i++) {
        for (let j = 0; j < n; j++) {
          cell(gx0 + i, gy0 + j, d[4]);
        }
      }
    }
    return canvas.toDataURL('image/png');
  } catch (e) {
    return 'DIAG:ERR:' + (e && e.message ? e.message : String(e));
  }
})()"#;

/// 采集页面调试信息（JSON 字符串）
const DEBUG_JS: &str = r#"(() => {
  return JSON.stringify({
    url: location.href,
    title: document.title,
    hasCanvas: !!document.querySelector('canvas'),
    canvasSize: (() => { const c = document.querySelector('canvas'); return c ? c.width + 'x' + c.height : ''; })(),
    imgCount: document.querySelectorAll('img').length,
    largeImgs: [...document.querySelectorAll('img')].filter(i => i.width >= 80 && i.height >= 80).length,
    loginBtns: [...document.querySelectorAll('button, a, [role="button"]')]
      .filter(el => /登录|登\s*录/i.test(el.textContent || '') && el.offsetParent !== null)
      .map(el => (el.textContent || '').trim().slice(0, 20)).slice(0, 5),
    visibleInputs: [...document.querySelectorAll('[contenteditable="true"], textarea')]
      .filter(e => e.offsetParent !== null).length,
    iframes: document.querySelectorAll('iframe').length,
    bodySnippet: (document.body ? document.body.innerText : '').slice(0, 300),
    cookieKeys: document.cookie.split(';').map(c => c.trim().split('=')[0]).filter(Boolean),
    cookieLen: document.cookie.length,
    localStorageKeys: (() => { try { return Object.keys(localStorage); } catch(e) { return []; } })(),
    hasLoginBtn: !!document.querySelector('[data-testid="to_login_button"]'),
  });
})()"#;

impl Doubao {
    pub fn new(browser: Browser, page: obscura::Page, cfg: Arc<ProviderCfg>) -> Self {
        Self {
            browser,
            page,
            debug_html: cfg.debug_html,
            cfg,
            qr_poll: Arc::new(std::sync::Mutex::new(None)),
            page_recreated: false,
            navigate_after_recreate: false,
            qr_token: Arc::new(std::sync::Mutex::new(None)),
            saved_qr_token: None,
            qr_refresh_retries: 0,
            cookie_tick: 0,
            reply_capture: Arc::new(std::sync::Mutex::new(None)),
            completion_state: Arc::new(std::sync::Mutex::new(CompletionState::None)),
            response_cb_id: None,
        }
    }

    /// 把 cookie jar 落盘（原子写）。obscura 启动时自动从该文件恢复，
    /// 重启后无需重新扫码（cookie 未过期时）。
    fn save_cookies(&self) {
        let Some(path) = &self.cfg.cookie_path else { return };
        match self.browser.cookies().save_to_file(path) {
            Ok(()) => {}
            Err(e) => tracing::warn!("保存 cookies 到 {} 失败: {e}", path.display()),
        }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let proxy_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            // 客户端池空闲超时必须短于 CDN 服务端 keep-alive（通常 ~10s）：
            // 懒加载 chunk 的零散请求间隔长，池中连接早被服务端关闭，
            // 复用死连接会得到 "error sending request"（HTTP 0）
            .pool_idle_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");

        // 拦截代理必须独立任务并发运行：若在 select 分支里同步处理，
        // handle_command（整个登录流程）执行期间拦截请求无人 resolve，
        // 页面所有 XHR 挂起 → 导航 30s 超时。
        let mut intercept_task = self.spawn_intercept_proxy(proxy_client.clone());

        // 启动即导航：否则首次 status/debug 评估的是空白页（有 cookie 时会误报未登录）
        if self.page.goto(&self.cfg.base_url).await.is_ok() {
            self.page.settle(3000).await;
        }

        // 心跳泵：obscura 没有后台事件循环，页面的 setTimeout/XHR 回调只在
        // settle() 期间推进。登录弹窗的扫码轮询是 1s setTimeout 链，若不持续
        // 泵循环，扫码后页面永远感知不到。每 250ms 泵 200ms，命令最多等待 250ms。
        let mut heartbeat = tokio::time::interval(Duration::from_millis(250));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            // 自愈重建 page 后网络回调全部丢失，需要重新注册
            if self.page_recreated {
                self.page_recreated = false;
                intercept_task.abort();
                intercept_task = self.spawn_intercept_proxy(proxy_client.clone());
                self.register_response_callback();
                // reset 换新页后回到站点恢复登录态（cookie 在存储层，无需重扫）
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
                    self.refresh_qr_files_if_token_changed();
                    // cookie 周期落盘：每 240 tick（60s）一次兜底；
                    // 扫码确认登录（qr_poll 捕获 login/confirmed）后 5s 内
                    // 加速保存一次，避免用户看到登录成功立刻重启丢失 cookie
                    self.cookie_tick = self.cookie_tick.wrapping_add(1);
                    let login_confirmed = self
                        .qr_poll
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|(s, ts)| {
                            (s == "login" || s == "confirmed")
                                && ts.elapsed() < Duration::from_secs(10)
                        });
                    if self.cookie_tick >= 240
                        || (login_confirmed && self.cookie_tick % 20 == 0)
                    {
                        self.save_cookies();
                    }
                }
            }
        }
        // 退出前最后落盘一次，保住本次会话期间的所有 cookie 更新
        self.save_cookies();
        intercept_task.abort();
    }

    /// 为当前 page 启动独立的拦截代理任务（内部并发处理每个请求）。
    fn spawn_intercept_proxy(&mut self, client: reqwest::Client) -> tokio::task::JoinHandle<()> {
        let mut intercept_rx = self.setup_page_network();
        let completion_state = self.completion_state.clone();
        tokio::spawn(async move {
            while let Some(req) = intercept_rx.recv().await {
                let client = client.clone();
                let completion_state = completion_state.clone();
                // 每个请求单独 spawn，避免慢请求阻塞队列
                tokio::spawn(async move {
                    Self::handle_intercept(req, &client, &completion_state).await;
                });
            }
        })
    }

    /// 在当前 page 上注册网络监控：
    /// 1) Script/Stylesheet 拦截（rustls 加载豆包 CDN 失败，用 reqwest 代理），返回接收端；
    /// 2) on_response 捕获 check_qrconnect 扫码轮询结果（ground truth）。
    fn setup_page_network(
        &mut self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<obscura::InterceptedRequest> {
        let intercept_rx = self.page.enable_interception();
        self.register_response_callback();
        intercept_rx
    }

    /// 注册 on_response 回调（扫码状态 + AI 回复捕获）。
    /// 每次调用前移除旧回调，避免泄漏。obscura 的 on_response 在 SSE
    /// 响应后可能停止触发，需在每次 do_ask 前重新注册。
    fn register_response_callback(&mut self) {
        if let Some(id) = self.response_cb_id.take() {
            self.page.off_response(id);
        }
        let qr_poll = self.qr_poll.clone();
        let qr_token = self.qr_token.clone();
        let reply_capture = self.reply_capture.clone();
        let completion_state = self.completion_state.clone();
        let id = self.page.on_response(Arc::new(
            move |_req: &obscura::RequestInfo, resp: &obscura::Response| {
                let url = resp.url.as_str();

                // /chat/completion 响应已完整到达（无论提取成功与否），
                // 网络路径生命周期结束
                if url.contains("/chat/completion") {
                    *completion_state.lock().unwrap() = CompletionState::Done;
                }

                // ---- 全量 URL 日志（调试用，找聊天 API 端点）----
                let ct = resp
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                let is_stream = ct.contains("event-stream");
                let is_chat_api = url.contains("/chat/")
                    && !url.contains("/static/")
                    && !url.contains("/favicon")
                    && !url.contains("/css/")
                    && !url.contains("/js/")
                    && !url.contains("/drive/")
                    && !url.contains("/cron")
                    && !url.contains("/skills");

                if url.contains("get_qrcode") {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&resp.body) {
                        let token = v
                            .get("data")
                            .and_then(|d| d.get("token"))
                            .and_then(|t| t.as_str())
                            .map(|t| t.to_string());
                        *qr_token.lock().unwrap() = token;
                    }
                }
                if url.contains("check_qrconnect") {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&resp.body) {
                        if let Some(status) = v
                            .get("data")
                            .and_then(|d| d.get("status"))
                            .and_then(|s| s.as_str())
                        {
                            let mut last = qr_poll.lock().unwrap();
                            *last = Some((status.to_string(), Instant::now()));
                        }
                    }
                    return;
                }

                // ---- AI 回复捕获（网络层 ground truth）----
                // DOM 渲染可能因 CDN JS 加载失败而缺失，直接从网络响应提取文本。
                if !is_stream && !is_chat_api {
                    return;
                }
                let body_str = String::from_utf8_lossy(&resp.body);
                // SSE 聊天响应原文落盘（html/ 目录，已 gitignore），便于分析
                // 豆包协议结构（思考块/搜索结果/参考资料等）
                if is_stream && body_str.contains("STREAM_") {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let path = std::path::Path::new("html").join(format!("{ts}_sse_chat.txt"));
                    if let Err(e) = std::fs::write(&path, body_str.as_bytes()) {
                        tracing::warn!("SSE 响应落盘失败: {e}");
                    }
                }
                let extracted = if is_stream {
                    extract_sse_reply(&body_str)
                } else {
                    extract_json_reply(&body_str)
                };
                if let Some(mut r) = extracted {
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
                    let qr_state = if logged { "none".to_string() } else { self.qr_state() };
                    let _ = reply_tx.send(StatusResponse {
                        logged_in: logged,
                        qr_state,
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
                            // 新 page 丢失网络拦截/响应捕获，主循环需重建
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
                    let info = self.debug_info();
                    let _ = reply_tx.send(info);
                }
                Command::DumpHtml { url, reply_tx } => {
                    if let Some(url) = url {
                        let _ = self.page.goto(&url).await;
                        self.page.settle(3000).await;
                    }
                    let path = self.dump_html();
                    let _ = reply_tx.send(path);
                }
                Command::SetDebugHtml { enabled, reply_tx } => {
                    self.debug_html = enabled.unwrap_or(!self.debug_html);
                    let _ = reply_tx.send(DebugHtmlResponse {
                        debug_html: self.debug_html,
                    });
                }
                Command::Screenshot { reply_tx } => {
                    let html = self.page.evaluate("document.documentElement.outerHTML").as_str().unwrap_or("").to_string();
                    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
                    let path = std::env::temp_dir().join(format!("doubao-screenshot-{ts}.html"));
                    match std::fs::write(&path, html.as_bytes()) {
                        Ok(_) => { let _ = reply_tx.send(Ok(path.to_string_lossy().to_string())); }
                        Err(e) => { let _ = reply_tx.send(Err(anyhow::anyhow!(e))); }
                    }
                }
                Command::ClickAt { x, y, reply_tx } => {
                    let js = format!(
                        r#"(() => {{ const el = document.elementFromPoint({x}, {y}); if (el) {{ el.click(); return true; }} return false; }})()"#
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
                    let _ = reply_tx.send(self.import_cookies(cookies).await);
                }
            }
    }

    /// 拦截 obscura 的 JS/CSS 请求，用 reqwest(native-tls) 代理获取
    async fn handle_intercept(
        req: obscura::InterceptedRequest,
        client: &reqwest::Client,
        completion_state: &Arc<std::sync::Mutex<CompletionState>>,
    ) {
        let rt = req.resource_type.as_str();
        let url_str = req.url.as_str();

        // /chat/completion 请求已发出：标记网络路径在途（wait_reply 据此
        // 等待网络层回复，而不是拿 DOM 上刚发出的问题回显当回复）
        if url_str.contains("/chat/completion") {
            let mut st = completion_state.lock().unwrap();
            if *st == CompletionState::None {
                *st = CompletionState::InFlight;
            }
        }

        // 专门处理 /chat/completion 请求：风控 403 可能由无效 fp 指纹导致。
        // 尝试移除 fp 参数看服务器是否接受。
        if url_str.contains("/chat/completion") && (url_str.contains("fp=") || url_str.contains("msToken=")) {
            let modified_url = {
                let qpos = url_str.find('?').unwrap_or(url_str.len());
                let base = &url_str[..qpos];
                let query = if qpos < url_str.len() { &url_str[qpos + 1..] } else { "" };
                let filtered: Vec<&str> = query
                    .split('&')
                    .filter(|p| !p.starts_with("fp=") && !p.starts_with("msToken=") && !p.starts_with("a_bogus="))
                    .collect();
                if filtered.is_empty() {
                    base.to_string()
                } else {
                    format!("{base}?{}", filtered.join("&"))
                }
            };
            let mut headers = req.headers.clone();
            if !headers.contains_key("content-type") {
                headers.insert("content-type".to_string(), "application/json".to_string());
            }
            let _ = req.resolver.send(InterceptResolution::Continue {
                url: Some(modified_url),
                method: None,
                headers: Some(headers),
                body: None,
            });
            return;
        }

        // 只代理 Script 和 Stylesheet（obscura 的 rustls 加载豆包 CDN 失败的类型）。
        // Fetch/XHR 必须原生透传：InterceptedRequest 不携带请求体，代理转发会丢掉
        // check_qrconnect 等接口放在 body 里的 token，导致扫码轮询永远失败。
        // 原生路径带 body/cookie/UA 直连 www.doubao.com（导航同主机 TLS 正常）。
        if rt != "Script" && rt != "Stylesheet" {
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
        // 空闲连接复用 / 单线程调度下推进慢）时重试，避免懒加载 chunk 丢失
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
                        "代理请求失败（将重试 {}/{}）: {url} → {e:#}",
                        attempt + 1,
                        PROXY_SEND_RETRIES
                    );
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                Err(e) => {
                    tracing::warn!("代理请求失败: {url} → {e:#}");
                }
            }
        }

        let Some(resp) = resp_opt else {
            let _ = req.resolver.send(InterceptResolution::Fail {
                reason: "proxy request error (retries exhausted)".into(),
            });
            return;
        };
        {
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
                    tracing::warn!("代理读取body失败: {url} → {e}");
                    let _ = req.resolver.send(InterceptResolution::Fail {
                        reason: format!("proxy body read error: {e}"),
                    });
                }
            }
        }
    }

    fn is_logged_in(&self) -> bool {
        self.page
            .evaluate(LOGGED_IN_JS)
            .as_bool()
            .unwrap_or(false)
    }

    /// 导入真实浏览器导出的 cookie 并校验登录态。
    /// CDP 授权（cdp_auth）拉起本地 Chrome 完成登录后，收集的 cookie
    /// 转成 EditThisCookie JSON 走这里注入；也兼容扩展手动导出的导入。
    /// 关键 cookie 是 `sessionid`（豆包会话凭证，登录后由站点轮换）。
    async fn import_cookies(&mut self, raw: String) -> anyhow::Result<ImportCookiesResponse> {
        let entries = parse_cookie_import(&raw);
        if entries.is_empty() {
            anyhow::bail!(
                "未解析到任何 cookie。支持格式：\
                 1) EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组；\
                 2) 每行一条 `name=value; Domain=doubao.com; Path=/` Set-Cookie 串"
            );
        }
        let has_session = entries.iter().any(|e| e.contains("sessionid="));
        let mut imported = 0usize;
        for entry in &entries {
            match self.browser.cookies().set(entry, &self.cfg.base_url) {
                Ok(()) => imported += 1,
                Err(e) => tracing::warn!("cookie 导入失败（{entry}）: {e}"),
            }
        }

        // 刷新页面验证登录态（sessionid 有效则不再出现登录入口）
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        let logged_in = self.is_logged_in();
        if logged_in {
            self.save_cookies();
        }
        let message = if logged_in {
            format!("导入 {imported} 条 cookie，登录态已恢复并持久化到 storage")
        } else if !has_session {
            format!(
                "导入 {imported} 条 cookie 但未登录：缺少关键 cookie `sessionid`\
                 （doubao.com 域，HttpOnly）。请确认导出时已登录豆包且导出的是\
                 doubao.com 的 cookie"
            )
        } else {
            format!(
                "导入 {imported} 条 cookie 但未登录：sessionid 可能已过期或在\
                 其他浏览器上被轮换。请重新登录豆包后重新导出导入"
            )
        };
        Ok(ImportCookiesResponse {
            imported,
            logged_in,
            message,
        })
    }

    /// 登录弹窗二维码状态：scanned / expired / ready / none。
    /// 优先用网络层轮询结果（ground truth，check_qrconnect 响应的 data.status），
    /// 8 秒内有新鲜轮询才采用；否则回退到 UI 文本检测。
    fn qr_state(&self) -> String {
        if let Some((status, ts)) = self.qr_poll.lock().unwrap().clone() {
            if ts.elapsed() < Duration::from_secs(8) {
                return match status.as_str() {
                    "new" => "ready".to_string(),
                    "scanned" | "confirmed" | "login" => "scanned".to_string(),
                    "expired" => "expired".to_string(),
                    // refused（手机端取消）等其余状态原样透出
                    other => other.to_string(),
                };
            }
        }
        self.page
            .evaluate(QR_STATE_JS)
            .as_str()
            .unwrap_or("none")
            .to_string()
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

    fn read_qr(&mut self) -> String {
        let qr = self
            .page
            .evaluate(EXTRACT_QR_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if !qr.is_empty() {
            return qr;
        }
        // 主文档没有则逐个搜索子 frame；
        // 跨域 iframe 拿不到 contentDocument，必须走 evaluate_in_frame
        let frame_count = self.page.frame_urls().len();
        for i in 0..frame_count {
            if let Ok(v) = self.page.evaluate_in_frame(i, EXTRACT_QR_JS) {
                let s = v.as_str().unwrap_or("").to_string();
                if !s.is_empty() {
                    return s;
                }
            }
        }
        qr
    }

    fn debug_info(&self) -> String {
        self.page
            .evaluate(DEBUG_JS)
            .as_str()
            .unwrap_or("{}")
            .to_string()
    }

    /// 把渲染后的页面 HTML 写入临时文件，返回文件路径
    fn dump_html(&self) -> String {
        dump_page_html(
            &self.page,
            &format!("ai-web-gateway-{}-page", self.cfg.name),
        )
    }

    /// 调试模式：把当前渲染后的页面 HTML 存到项目目录 `html/` 下（common 实现）
    fn trace_html(&self, step: &str) {
        trace_page_html(&self.page, self.debug_html, step);
    }

    /// SVG→PNG 规范重建：把圆点样式 QR 还原成标准方点 QR（含静区），
    /// 任何扫码器都能识别；结构识别失败返回 DIAG: 诊断串，调用方退回 SVG。
    fn convert_qr_to_png(&self) -> String {
        let png = self
            .page
            .evaluate(RENDER_QR_PNG_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if png.starts_with("DIAG:") {
            tracing::warn!("SVG→PNG 重绘诊断: {png}");
            return String::new();
        }
        if png.is_empty() {
            tracing::warn!("SVG→PNG 重绘失败（未找到二维码 SVG），退回 SVG");
        }
        png
    }

    /// 二维码 60s 过期后页面自动换新 token（get_qrcode 重新请求）。
    /// 保存的二维码文件必须跟着重存，否则用户扫到已失效的旧码。
    /// 由心跳调用：token 变化即从 DOM 重新提取二维码并覆盖保存文件。
    fn refresh_qr_files_if_token_changed(&mut self) {
        let current = self.qr_token.lock().unwrap().clone();
        let Some(current) = current else { return };
        if self.saved_qr_token.as_deref() == Some(current.as_str()) {
            self.qr_refresh_retries = 0;
            return;
        }
        if self.qr_refresh_retries >= 12 {
            // DOM 迟迟不配合（弹窗已关闭等），放弃本轮，等下次 token 变化
            tracing::warn!(
                "二维码 token 已变化但多次提取失败，放弃本轮重存（token {:.12}…）",
                current
            );
            self.saved_qr_token = Some(current);
            self.qr_refresh_retries = 0;
            return;
        }
        let qr = self.read_qr();
        if qr.is_empty() {
            self.qr_refresh_retries += 1;
            return;
        }
        let png = self.convert_qr_to_png();
        let final_qr = if !png.is_empty() { png } else { qr };
        let _ = save_qr_image(
            "豆包",
            "doubao_login_qr",
            &final_qr,
            "用抖音 / 头条 App 扫描上方二维码完成登录",
        );
        self.saved_qr_token = Some(current.clone());
        self.qr_refresh_retries = 0;
    }

    /// 获取登录二维码。失败时（页面长时间运行后会劣化：JS 不再执行、DOM 停滞，
    /// 表现为找不到登录按钮、body 只剩未水合脚本文本）自动重建浏览器页面重试一次。
    async fn login_qr(&mut self) -> anyhow::Result<QrResponse> {
        for attempt in 1..=2u8 {
            match self.login_qr_attempt().await {
                Ok(Some(r)) => return Ok(r),
                // 页面导航后确认已登录，无需二维码
                Ok(None) => anyhow::bail!("已登录，无需获取二维码"),
                Err(e) => {
                    if attempt == 1 {
                        tracing::warn!("获取二维码失败（{e:#}），重建浏览器页面后重试一次");
                        self.page = self
                            .browser
                            .new_page()
                            .await
                            .map_err(|ne| anyhow::anyhow!("重建页面失败: {ne}；原始错误: {e:#}"))?;
                        // 新 page 丢失网络回调，通知 run() 主循环重新注册
                        self.page_recreated = true;
                    } else {
                        return Err(anyhow::anyhow!("重建页面后重试仍未获取到二维码: {e:#}"));
                    }
                }
            }
        }
        unreachable!("login_qr 重试循环必然返回")
    }

    /// 单次登录二维码获取尝试。Ok(None) 表示页面已登录无需二维码。
    async fn login_qr_attempt(&mut self) -> anyhow::Result<Option<QrResponse>> {
        // 清空上一轮扫码的轮询缓存，避免旧 token 的 expired 残留 8 秒误导 qr_state
        *self.qr_poll.lock().unwrap() = None;
        // 始终导航到豆包页面（不依赖 is_logged_in，避免误判跳过）
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        self.trace_html("login_qr_1_after_goto");

        if self.is_logged_in() {
            // 页面水合慢时「登录」按钮可能还没渲染，输入框却已可见，会误判成已登录；
            // 等 3s 让 SSR 水合完成再复检一次
            self.page.settle(3000).await;
            if self.is_logged_in() {
                return Ok(None);
            }
            tracing::warn!("登录态初判为已登录但复检为未登录（页面水合慢），继续获取二维码");
        } else {
            // SSR 水合慢时输入框可能还没渲染，初判为未登录会误触发二维码流程；
            // 等 3s 让水合完成再复检，确认真的未登录才去找二维码
            self.page.settle(3000).await;
            if self.is_logged_in() {
                return Ok(None);
            }
        }

        // 轮询最多 30s：先给自动弹窗/React 水合留 4s；二维码未出现且登录弹窗没开时
        // 每隔 5s 重点一次「登录」（SSR 按钮在水合完成前点不动，bundle 加载慢时
        // 水合可能超过 20s，必须持续重试）；弹窗已打开则纯轮询等二维码渲染，绝不重点
        let start = Instant::now();
        let mut next_click_ms: u128 = 4000;
        let mut reloaded = false;
        loop {
            let qr = self.read_qr();
            if !qr.is_empty() {
                // 尝试 SVG→PNG 转换（PNG 更通用，所有查看器都能显示）
                let png = self.convert_qr_to_png();
                let final_qr = if !png.is_empty() { png } else { qr };
                let qr_file = save_qr_image(
                    "豆包",
                    "doubao_login_qr",
                    &final_qr,
                    "用抖音 / 头条 App 扫描上方二维码完成登录",
                );
                // 记住本 token，心跳检测到页面自动刷新（新 token）时才重存文件
                self.saved_qr_token = self.qr_token.lock().unwrap().clone();
                self.qr_refresh_retries = 0;
                self.trace_html("login_qr_3_qr_ready");
                return Ok(Some(QrResponse {
                    qr_data_url: final_qr,
                    qr_file,
                    tip: "用抖音/飞书 App 扫描本次返回的二维码（历史保存的二维码文件已失效，\
                          页面会自动刷新二维码，每次 login 都是新 token），\
                          然后轮询 GET /api/doubao/status：qr_state=scanned 表示已扫码待确认，\
                          logged_in=true 表示登录完成"
                        .into(),
                }));
            }

            let elapsed = start.elapsed().as_millis();
            // 兜底：点击多次弹窗仍没出现（水合卡死/风控接口挂起），整页刷新重来一次
            if elapsed > 18000 && !reloaded {
                reloaded = true;
                tracing::warn!("弹窗迟迟未出现，整页刷新后重试");
                self.page.goto(&self.cfg.base_url).await?;
                self.page.settle(3000).await;
                self.trace_html("login_qr_5_after_reload");
                next_click_ms = 0; // 刷新后立即重新走点击流程
                continue;
            }
            if elapsed >= next_click_ms {
                let dialog_open = self
                    .page
                    .evaluate(DIALOG_OPEN_JS)
                    .as_bool()
                    .unwrap_or(false);
                if !dialog_open {
                    // 先关掉挡路的推广弹窗（「下载豆包电脑版」等，遮罩会拦截登录按钮点击）
                    let promo_closed = self
                        .page
                        .evaluate(CLOSE_PROMO_JS)
                        .as_bool()
                        .unwrap_or(false);
                    if promo_closed {
                        self.page.settle(1000).await;
                    }
                    next_click_ms = elapsed + 5000;
                    let res = self
                        .page
                        .evaluate(CLICK_LOGIN_JS)
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    if !res.starts_with("CLICKED:") {
                        tracing::warn!("未找到可点击的「登录」按钮（{res}），继续轮询二维码");
                    }
                    self.page.settle(2000).await;
                    self.trace_html("login_qr_2_after_click_login");
                    continue;
                }
            }
            if elapsed > 45000 {
                break;
            }
            self.page.settle(1000).await;
        }

        // 输出调试信息辅助排查
        self.trace_html("login_qr_4_failed");
        let info = self.debug_info();
        tracing::error!("登录页调试信息: {info}");
        let html_path = self.dump_html();
        anyhow::bail!(
            "45s 内未捕获到登录二维码。调试信息: {info}。HTML 快照: {html_path}。\
             可 GET /api/doubao/debug 实时查看登录页状态；若豆包改版需更新 EXTRACT_QR_JS 选择器"
        );
    }

    /// 手机号登录第一步：导航 → 点登录按钮 → 切换到手机号登录 → 填入手机号 → 发送验证码
    async fn login_phone(&mut self, phone: &str) -> anyhow::Result<LoginPhoneResponse> {
        self.page.goto(&self.cfg.base_url).await?;
        self.page.settle(3000).await;
        self.trace_html("login_phone_1_after_goto");

        if self.is_logged_in() {
            anyhow::bail!("已登录，无需再次登录");
        }

        // 先关掉挡路的推广弹窗（遮罩会拦截登录按钮点击），再点右上角「登录」按钮唤出弹窗
        if self
            .page
            .evaluate(CLOSE_PROMO_JS)
            .as_bool()
            .unwrap_or(false)
        {
            self.page.settle(1000).await;
        }
        let res = self
            .page
            .evaluate(CLICK_LOGIN_JS)
            .as_str()
            .unwrap_or("")
            .to_string();
        if !res.starts_with("CLICKED:") {
            tracing::warn!("未找到「登录」按钮（{res}），可能弹窗已打开");
        }
        self.page.settle(2000).await;
        self.trace_html("login_phone_2_after_click_login");

        // 切换到「手机号登录」标签
        let switched = self
            .page
            .evaluate(SWITCH_PHONE_TAB_JS)
            .as_bool()
            .unwrap_or(false);
        if !switched {
            tracing::warn!("未找到「手机号登录」标签，可能已在手机号登录页面");
        }
        self.page.settle(1500).await;
        self.trace_html("login_phone_3_after_switch_tab");

        // 填入手机号
        let phone_json = serde_json::to_string(phone)?;
        let fill_js = format!("{FILL_PHONE_JS}({phone_json})");
        let filled = self.page.evaluate(&fill_js).as_bool().unwrap_or(false);
        if !filled {
            let info = self.debug_info();
            let html_path = self.dump_html();
            anyhow::bail!(
                "未找到手机号输入框。调试信息: {info}。HTML 快照: {html_path}。\
                 可 GET /api/doubao/debug 查看页面状态"
            );
        }
        self.page.settle(500).await;
        self.trace_html("login_phone_4_after_fill_phone");

        // 点击「发送验证码」
        let sent = self
            .page
            .evaluate(SEND_CODE_JS)
            .as_bool()
            .unwrap_or(false);
        if !sent {
            let info = self.debug_info();
            let html_path = self.dump_html();
            anyhow::bail!(
                "未找到「发送验证码」按钮。调试信息: {info}。HTML 快照: {html_path}。\
                 可 GET /api/doubao/debug 查看页面状态"
            );
        }
        self.page.settle(500).await;
        self.trace_html("login_phone_5_after_send_code");

        Ok(LoginPhoneResponse {
            tip: "验证码已发送，请调用 POST /api/doubao/login/verify 提交验证码完成登录".into(),
        })
    }

    /// 手机号登录第二步：填入验证码 → 点击登录 → 等待登录成功
    async fn login_verify(&mut self, code: &str) -> anyhow::Result<LoginVerifyResponse> {
        // 填入验证码
        let code_json = serde_json::to_string(code)?;
        let fill_js = format!("{FILL_CODE_JS}({code_json})");
        let filled = self.page.evaluate(&fill_js).as_bool().unwrap_or(false);
        if !filled {
            let info = self.debug_info();
            let html_path = self.dump_html();
            anyhow::bail!(
                "未找到验证码输入框。调试信息: {info}。HTML 快照: {html_path}。\
                 可 GET /api/doubao/debug 查看页面状态"
            );
        }
        self.page.settle(500).await;
        self.trace_html("login_verify_1_after_fill_code");

        // 点击登录按钮
        let clicked = self
            .page
            .evaluate(CLICK_PHONE_LOGIN_BTN_JS)
            .as_bool()
            .unwrap_or(false);
        if !clicked {
            let info = self.debug_info();
            let html_path = self.dump_html();
            anyhow::bail!(
                "未找到登录按钮。调试信息: {info}。HTML 快照: {html_path}。\
                 可 GET /api/doubao/debug 查看页面状态"
            );
        }
        self.page.settle(500).await;
        self.trace_html("login_verify_2_after_click_login");

        // 等待登录成功（最多 15s）
        let start = Instant::now();
        loop {
            self.page.settle(1000).await;
            if self.is_logged_in() {
                self.trace_html("login_verify_3_logged_in");
                self.save_cookies();
                return Ok(LoginVerifyResponse { logged_in: true });
            }
            if start.elapsed().as_millis() > 15000 {
                self.trace_html("login_verify_4_timeout");
                let info = self.debug_info();
                let html_path = self.dump_html();
                anyhow::bail!(
                    "15s 内未完成登录。调试信息: {info}。HTML 快照: {html_path}。\
                     可能验证码错误或已过期"
                );
            }
        }
    }

    /// 找到输入框 → 填入 → 发送（优先点发送按钮，失败退化为派发 Enter 键）
    /// 等聊天输入框（contenteditable 编辑器）渲染完成。
    /// CDN JS 代理失败时输入框可能长期缺失（只剩搜索框等 textarea），
    /// 第一轮超时后重载页面再等一轮。
    async fn wait_for_chat_input(&mut self) -> bool {
        const HAS_INPUT_JS: &str =
            r#"!!document.querySelector('.ProseMirror[contenteditable="true"], [contenteditable="true"]')"#;
        for round in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(if round == 0 { 10 } else { 15 });
            while Instant::now() < deadline {
                if self.page.evaluate(HAS_INPUT_JS).as_bool().unwrap_or(false) {
                    return true;
                }
                self.page.settle(500).await;
            }
            if round == 0 {
                tracing::warn!("聊天输入框未渲染（CDN JS 加载失败？），重载页面重试");
                if self.page.goto(&self.cfg.base_url).await.is_err() {
                    return false;
                }
            }
        }
        false
    }

    /// 等发送按钮出现且可用（编辑器状态同步信号），超时返回 false。
    async fn wait_send_button(&mut self, timeout_ms: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            if self
                .page
                .evaluate(SEND_BTN_READY_JS)
                .as_bool()
                .unwrap_or(false)
            {
                return true;
            }
            self.page.settle(300).await;
        }
        false
    }

    async fn send_message(&mut self, text: &str) -> anyhow::Result<()> {
        // 等输入框渲染（CDN JS 失败时可能缺失，内部重载一次重试）
        if !self.wait_for_chat_input().await {
            anyhow::bail!("聊天输入框未渲染（页面 CDN JS 加载失败），请稍后重试 ask");
        }
        // 填入问题：common::fill 多策略（ProseMirror view.dispatch / tiptap /
        // beforeinput / paste / execCommand / React fiber / DOM 兜底），
        // 返回 ok-* 表示填入并通过 DOM 事后校验
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
        // 豆包聊天输入框是 contenteditable 编辑器；ok-textarea 说明 fill 兜底
        // 找错了元素（搜索框等），重载页面后再试一次
        if fill_result == "ok-textarea" {
            tracing::warn!("填到了非聊天输入框（{fill_result}），重载页面后重试填入");
            self.page.goto(&self.cfg.base_url).await?;
            self.page.settle(2000).await;
            if !self.wait_for_chat_input().await {
                anyhow::bail!("聊天输入框未渲染（页面 CDN JS 加载失败），请稍后重试 ask");
            }
            let retry = self
                .page
                .evaluate(&fill_js)
                .as_str()
                .unwrap_or("")
                .to_string();
            if !retry.starts_with("ok-") || retry == "ok-textarea" {
                anyhow::bail!("问题文本未能写入聊天输入框（{retry}），页面可能未完全加载");
            }
        } else if !fill_result.starts_with("ok-") {
            anyhow::bail!("问题文本未写入输入框（{fill_result}），输入可能被页面重置");
        }
        self.page.settle(1000).await;
        self.trace_html("ask_1_after_fill_question");

        // 豆包只在编辑器状态非空时渲染 chat_input_send_button，按钮出现即
        // 证明 fill 文本已同步进编辑器内部状态。fill 的 ok-dom 兜底策略只改
        // DOM 不同步状态：PM 重渲染会抹掉文本，按钮不出现，此时发送会空发
        // （enter 无文本可发 → 无 completion 请求 → 整轮 ask 超时）。
        // 等不到按钮：清空编辑器重填一次，仍失败立即报精确错误。
        if !self.wait_send_button(4000).await {
            tracing::warn!("发送按钮未出现（fill={fill_result}），编辑器状态可能未同步，清空重填");
            let _ = self
                .page
                .evaluate(CLEAR_EDITOR_JS)
                .as_str()
                .unwrap_or("")
                .to_string();
            let refilled = self
                .page
                .evaluate(&fill_js)
                .as_str()
                .unwrap_or("")
                .to_string();
            if !refilled.starts_with("ok-") {
                anyhow::bail!("问题文本未能写入聊天输入框（重填={refilled}），页面可能未完全加载");
            }
            if !self.wait_send_button(4000).await {
                anyhow::bail!(
                    "发送按钮未出现：填入未同步到豆包编辑器状态（fill={fill_result} 重填={refilled}），请重试 ask"
                );
            }
        }

        // 发送：优先用 data-testid 精确找发送按钮；
        // 按钮可能 disabled（ProseMirror 状态未同步），退化为 Enter 键。
        let sent = self
            .page
            .evaluate(
                r#"(() => {
  // 优先用 data-testid 精确匹配
  let btn = document.querySelector('[data-testid="chat_input_send_button"]');
  if (!btn || btn.disabled) {
    btn = [...document.querySelectorAll('button')].find(b =>
      !b.disabled && (
        /发送/.test(b.getAttribute('aria-label') || '') ||
        /发送/.test(b.title || '') ||
        /发送/.test(b.textContent || '')
      )
    );
  }
  if (btn && !btn.disabled) { btn.click(); return 'button'; }
  // 按钮不存在或 disabled，尝试 Enter 键；编辑器为空说明填入未同步，快速失败
  const el = document.querySelector('[contenteditable="true"], textarea');
  if (el) {
    if (!(el.textContent || el.value || '').trim()) return 'empty';
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
        if sent == "empty" {
            anyhow::bail!("发送失败：编辑器为空（填入的文本未同步进编辑器状态），请重试 ask");
        }
        self.page.settle(500).await;

        // 发送结果校验：豆包发送成功会清空输入框；若问题文本仍在输入框，
        // 说明 enter/按钮没触发页面发送逻辑（如风控点击无响应、React 状态
        // 未同步），再重试一次，仍失败则报错而不是拿旧内容当回复
        let input_still_has_text_js = format!(
            r#"((t) => {{
  const el = document.querySelector('.ProseMirror[contenteditable="true"], [contenteditable="true"]');
  return !!el && (el.textContent || '').trim().includes(t);
}})({})"#,
            serde_json::to_string(text)?
        );
        let mut not_sent = self
            .page
            .evaluate(&input_still_has_text_js)
            .as_bool()
            .unwrap_or(false);
        if not_sent {
            tracing::warn!("首次发送未生效（输入框未清空），重试一次");
            self.page
                .evaluate(
                    r#"(() => {
  const btn = document.querySelector('[data-testid="chat_input_send_button"]');
  if (btn && !btn.disabled) { btn.click(); return 'button'; }
  const el = document.querySelector('[contenteditable="true"]');
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
            anyhow::bail!("发送未生效（输入框文本未被发出，页面可能被风控或未完全加载），请稍后重试");
        }
        self.trace_html("ask_2_after_send");
        Ok(())
    }

    /// 等回复说完；流式模式通过 chunk_tx 下发增量分片。
    ///
    /// 等待策略（网络层是 ground truth）：
    /// 1. 等 /chat/completion 请求发出（completion_state 离开 None），
    ///    最多 10s 宽限；发出后等网络层捕获（响应完整到达即提取完成）。
    /// 2. completion 响应已到达但提取失败（Done 且无捕获），或 10s 内
    ///    根本没看到 completion 请求（拦截丢失等），退回 DOM 稳定性等待。
    ///    DOM 路径会忽略与所提问题相同的文本（页面渲染的问题回显）。
    async fn wait_reply(
        &mut self,
        question: &str,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(self.cfg.max_wait_ms);
        // 问题文本的空白归一化形式：用于识别 DOM 上的问题回显
        let q_norm: String = question.chars().filter(|c| !c.is_whitespace()).collect();

        // 阶段1：宽限期内等 completion 请求发出；期间捕获若先到直接返回
        let mut network_engaged = false;
        while Instant::now() < deadline {
            let captured_opt = self.reply_capture.lock().unwrap().clone();
            if let Some((captured, _ts)) = captured_opt {
                if !captured.reply.is_empty() || !captured.thinking.is_empty() {
                    return Ok(self.finish_with_capture(captured, chunk_tx).await);
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

        // 阶段2：completion 在途，等网络层捕获（响应到达时同步写入）
        if network_engaged {
            while Instant::now() < deadline {
                let captured_opt = self.reply_capture.lock().unwrap().clone();
                if let Some((captured, _ts)) = captured_opt {
                    if !captured.reply.is_empty() || !captured.thinking.is_empty() {
                        return Ok(self.finish_with_capture(captured, chunk_tx).await);
                    }
                }
                // 响应已完整到达但提取不到内容（协议变化等），退回 DOM 兜底
                if *self.completion_state.lock().unwrap() == CompletionState::Done {
                    tracing::warn!("completion 响应已到达但网络层未提取到回复，退回 DOM 兜底");
                    break;
                }
                self.page.settle(500).await;
            }
            if Instant::now() >= deadline {
                anyhow::bail!("{}s 内网络层未捕获到回复（completion 响应未完成），可能被风控", self.cfg.max_wait_ms / 1000);
            }
        } else {
            tracing::warn!("未检测到 completion 请求（拦截丢失或发送未生效），退回 DOM 兜底");
        }

        // 阶段3：DOM 稳定性等待（拿不到思考过程，thinking 为空）
        let mut last = self.dom_reply_text(&q_norm);
        self.trace_html("ask_3_first_reply");

        // DOM 增量是否已流出：捕获晚到时避免整段重发造成流式内容重复
        let mut streamed_dom = false;
        let mut stable_since = Instant::now();
        loop {
            // 每次迭代优先检查网络层捕获（ground truth）
            if let Some((captured, _ts)) = self.reply_capture.lock().unwrap().clone() {
                if !captured.reply.is_empty() {
                    if !streamed_dom {
                        if let Some(tx) = &chunk_tx {
                            if !captured.thinking.is_empty() {
                                let _ = tx
                                    .send(encode_chunk(ChunkKind::Thinking, &captured.thinking))
                                    .await;
                            }
                            let _ = tx
                                .send(encode_chunk(ChunkKind::Text, &captured.reply))
                                .await;
                        }
                    }
                    self.trace_html("ask_4_reply_done");
                    return Ok(captured);
                }
            }
            self.page.settle(1000).await;
            let cur = self.dom_reply_text(&q_norm);
            if cur != last {
                if let Some(tx) = &chunk_tx {
                    // 发送增量（豆包基本是追加式输出）
                    let delta = if cur.starts_with(&last) {
                        cur[last.len()..].to_string()
                    } else {
                        cur.clone()
                    };
                    let _ = tx.send(encode_chunk(ChunkKind::Text, &delta)).await;
                    streamed_dom = true;
                }
                last = cur;
                stable_since = Instant::now();
                continue;
            }
            // DOM 有真实内容且已稳定才算说完；空内容不能靠稳定超时返回
            let stopping = self.is_stopping();
            let has_content = !last.is_empty();
            if has_content && !stopping && stable_since.elapsed().as_millis() as u64 >= self.cfg.stable_ms {
                break;
            }
            if Instant::now() > deadline {
                break;
            }
        }
        // 稳定性 break 后最后再查一次网络层（capture 可能在 DOM 稳定判定的间隙到达）
        if let Some((captured, _ts)) = self.reply_capture.lock().unwrap().clone() {
            if !captured.reply.is_empty() {
                self.trace_html("ask_4_reply_done");
                return Ok(captured);
            }
        }
        if last.is_empty() {
            anyhow::bail!("等待回复超时（网络层与 DOM 均无有效内容），可能被风控或选择器失效");
        }
        self.trace_html("ask_4_reply_done");
        Ok(AiReply {
            thinking: String::new(),
            reply: last,
            search_queries: Vec::new(),
            references: Vec::new(),
        })
    }

    /// 网络层捕获到手后的收尾：流式下发、DOM 补答案、落 trace
    async fn finish_with_capture(
        &mut self,
        captured: AiReply,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> AiReply {
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
        self.trace_html("ask_3_net_reply");
        // 网络层是 ground truth，优先使用；DOM 仅作答案回退
        self.page.settle(2000).await;
        let mut final_reply = captured;
        if final_reply.reply.is_empty() {
            let dom_text = self.read_last();
            if !dom_text.is_empty() && !dom_text.contains("今天") {
                final_reply.reply = dom_text;
            }
        }
        self.trace_html("ask_4_reply_done");
        final_reply
    }

    /// DOM 路径的回复文本：read_last 并过滤掉问题回显
    /// （豆包会把刚发出的问题渲染在消息区，归一化空白后与问题相同即视为回显）
    fn dom_reply_text(&self, q_norm: &str) -> String {
        let dom = self.read_last();
        if dom.is_empty() || dom.contains("今天") {
            return String::new();
        }
        let dom_norm: String = dom.chars().filter(|c| !c.is_whitespace()).collect();
        if !q_norm.is_empty() && dom_norm == *q_norm {
            return String::new();
        }
        dom
    }

    /// 有历史对话则进入最近的一个（会话列表第一项，按更新时间排序），返回对话 id。
    /// 无对话（新账号）返回 None，由 send_message 在 guidance 页发送并创建新对话。
    async fn enter_latest_conversation(&mut self) -> Option<String> {
        let res = self
            .page
            .evaluate(
                r#"(() => {
  const items = document.querySelectorAll('[data-testid="conversation-list-v2-item"]');
  if (!items.length) return 'none';
  const first = items[0];
  const id = first.getAttribute('data-conversation-id') || '?';
  // 点击外层 <a>（React Router Link，合成 click 触发内部路由切换）
  (first.closest('a') || first).click();
  return 'clicked:' + id;
})()"#,
            )
            .as_str()
            .unwrap_or("")
            .to_string();
        let id = res.strip_prefix("clicked:")?.to_string();
        // 等路由切换 + 输入框渲染
        self.page.settle(2000).await;
        let entered = self
            .page
            .evaluate(
                // 必须等 contenteditable 聊天输入框渲染（豆包输入框是 ProseMirror
                // 编辑器）；只认 textarea 会把搜索框等误当就绪，导致后续填错元素
                r#"location.pathname.includes('/') &&
                   location.pathname !== '/chat' &&
                   !!document.querySelector('[contenteditable="true"]')"#,
            )
            .as_bool()
            .unwrap_or(false);
        if entered {
            return Some(id);
        }
        // 合成 click 未触发路由切换时兜底：直接导航到对话 URL
        let base = self.cfg.base_url.trim_end_matches('/');
        let url = format!("{base}/{id}");
        if self.page.goto(&url).await.is_err() {
            return None;
        }
        self.page.settle(3000).await;
        Some(id)
    }

    async fn do_ask(
        &mut self,
        question: String,
        chunk_tx: Option<mpsc::Sender<String>>,
    ) -> anyhow::Result<AiReply> {
        if !self.is_logged_in() {
            anyhow::bail!(
                "{}尚未登录，请到「网页版账号」页对豆包重新授权\
                 （CDP 浏览器登录，或短信/扫码登录）",
                crate::models::ERR_NOT_LOGGED_IN
            );
        }
        // 清除上一轮的网络层回复捕获与 completion 生命周期，重新注册 on_response 回调
        *self.reply_capture.lock().unwrap() = None;
        *self.completion_state.lock().unwrap() = CompletionState::None;
        self.register_response_callback();
        // 有历史对话则进入最近的一个提问；没有才创建新对话
        self.enter_latest_conversation().await;
        self.send_message(&question).await?;
        let reply = self.wait_reply(&question, chunk_tx).await?;
        Ok(reply)
    }
}

// ---- 豆包 AI 回复提取（SSE / JSON）----

/// 从 content_block 数组中提取回复：block_type==10000 是最终答案文本块，
/// 10025 是搜索结果块（queries=搜索关键字，results[].text_card=参考资料），
/// 其余 block_type 是思考过程块
fn extract_content_blocks(blocks: &[serde_json::Value]) -> Option<AiReply> {
    let mut answer = String::new();
    let mut thinking = String::new();
    let mut queries: Vec<String> = Vec::new();
    let mut references: Vec<Reference> = Vec::new();
    for block in blocks {
        let bt = block
            .get("block_type")
            .and_then(|t| t.as_i64())
            .unwrap_or(0);
        if bt == 10025 {
            // 搜索结果块：search_query_result_block.queries 是搜索关键字，
            // results[].text_card.{title,url} 是参考资料
            if let Some(sqrb) = block
                .get("content")
                .and_then(|c| c.get("search_query_result_block"))
            {
                if let Some(qs) = sqrb.get("queries").and_then(|q| q.as_array()) {
                    for q in qs {
                        if let Some(s) = q.as_str() {
                            if !s.is_empty() && !queries.iter().any(|e| e == s) {
                                queries.push(s.to_string());
                            }
                        }
                    }
                }
                if let Some(results) = sqrb.get("results").and_then(|r| r.as_array()) {
                    for r in results {
                        let Some(tc) = r.get("text_card") else { continue };
                        let url = tc.get("url").and_then(|u| u.as_str()).unwrap_or("");
                        if url.is_empty() || !url.starts_with("http") {
                            continue;
                        }
                        let title = tc.get("title").and_then(|t| t.as_str()).unwrap_or(url);
                        if !references.iter().any(|e| e.url == url) {
                            references.push(Reference {
                                title: title.to_string(),
                                url: url.to_string(),
                            });
                        }
                    }
                }
            }
            continue;
        }
        let Some(text) = block.get("content").and_then(block_text_value) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let target = if bt == 10000 { &mut answer } else { &mut thinking };
        if text.starts_with(target.as_str()) {
            *target = text;
        } else {
            target.push_str(&text);
        }
    }
    if answer.is_empty() && thinking.is_empty() && queries.is_empty() && references.is_empty() {
        None
    } else {
        Some(AiReply {
            thinking,
            reply: answer,
            search_queries: queries,
            references,
        })
    }
}

/// 处理单个豆包 SSE 事件（FULL_MSG_NOTIFY / STREAM_MSG_NOTIFY / STREAM_CHUNK / CHUNK_DELTA）
fn process_sse_event(event: &str, data: &str, reply: &mut AiReply) {
    if data.is_empty() {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return;
    };
    match event {
        "FULL_MSG_NOTIFY" => {
            let Some(message) = v.get("message") else {
                return;
            };
            let ut = message
                .get("user_type")
                .and_then(|t| t.as_i64())
                .unwrap_or(0);
            if ut == 1 {
                return;
            }
            if let Some(blocks) = message.get("content_block").and_then(|b| b.as_array()) {
                if let Some(r) = extract_content_blocks(blocks) {
                    merge_ai(reply, r);
                }
            }
        }
        "STREAM_MSG_NOTIFY" => {
            let ut = v
                .get("meta")
                .and_then(|m| m.get("user_type"))
                .and_then(|t| t.as_i64())
                .unwrap_or(0);
            if ut == 1 {
                return;
            }
            if let Some(blocks) = v
                .get("content")
                .and_then(|c| c.get("content_block"))
                .and_then(|b| b.as_array())
            {
                if let Some(r) = extract_content_blocks(blocks) {
                    merge_ai(reply, r);
                }
            }
        }
        "STREAM_CHUNK" => {
            if let Some(ops) = v.get("patch_op").and_then(|o| o.as_array()) {
                for op in ops {
                    if let Some(blocks) = op
                        .get("patch_value")
                        .and_then(|pv| pv.get("content_block"))
                        .and_then(|b| b.as_array())
                    {
                        if let Some(r) = extract_content_blocks(blocks) {
                            merge_ai(reply, r);
                        }
                    }
                }
            }
        }
        "CHUNK_DELTA" => {
            // 格式: {"text":"😊"} — 增量文本，直接追加到答案
            if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
                if !t.is_empty() {
                    merge_ai(
                        reply,
                        AiReply {
                            thinking: String::new(),
                            reply: t.to_string(),
                            search_queries: Vec::new(),
                            references: Vec::new(),
                        },
                    );
                }
            }
        }
        _ => {}
    }
}

/// 从 SSE 流式响应体中提取 AI 回复（思考过程 + 最终答案）。
/// 豆包 SSE 事件类型:
///   SSE_HEARTBEAT — 心跳，跳过
///   SSE_ACK       — 确认，跳过
///   FULL_MSG_NOTIFY — 完整消息（user_type=1 用户问题 / user_type=2 AI回复）
///   STREAM_MSG_NOTIFY — 流式消息（meta.user_type=2 AI回复，content.content_block）
///   STREAM_CHUNK  — 增量补丁（patch_op[].patch_value.content_block）
///   SSE_REPLY_END — 结束
fn extract_sse_reply(body: &str) -> Option<AiReply> {
    let mut reply = AiReply::default();
    for (event, data) in parse_sse_events(body) {
        process_sse_event(&event, &data, &mut reply);
    }
    if reply.reply.is_empty() && reply.thinking.is_empty() {
        None
    } else {
        Some(reply)
    }
}

/// 从 JSON 响应体中提取 AI 回复文本（JSON 路径无思考过程分离，全部作为答案）
fn extract_json_reply(body: &str) -> Option<AiReply> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        return json_find_text(&v).map(|text| AiReply {
            thinking: String::new(),
            reply: text,
            search_queries: Vec::new(),
            references: Vec::new(),
        });
    }
    None
}
