//! CDP 授权：拉起本地真实 Chrome（独立 user-data-dir + 随机远程调试端口），
//! 用户在真实浏览器里完成平台登录（扫码 / 手机号验证码 / Google OAuth，
//! 均可过真人风控）。授权窗口开启期间**持续同步凭证**：登录/聊天产生的
//! cookie 与 localStorage（千问发消息后才下发的 x5sec 风控凭证、DeepSeek
//! userToken 轮换等）一旦变化立即收集，复用 `Command::ImportCookies`
//! 注入 obscura 引擎并校验登录态、持久化——用户在窗口里的任何凭证
//! 变化都不会丢失；关闭窗口 / 超时 / 手动完成时凭证记录已就绪。
//!
//! 本模块不依赖 obscura（V8 !Send），全部是普通 async 代码，
//! 可在任意 tokio multi-thread runtime 上运行（网关 actix runtime 直接调用）。
//!
//! 典型流程（vortex-gateway 侧编排）：
//! ```no_run
//! # use ai_web_provider::cdp_auth::CdpLoginManager;
//! # use ai_web_provider::models::Command;
//! # async fn demo(tx: tokio::sync::mpsc::Sender<Command>) -> anyhow::Result<()> {
//! let mgr = CdpLoginManager::new();
//! // 拉起 Chrome 打开豆包登录页，用户扫码
//! mgr.start("doubao", "https://www.doubao.com/chat/", tx).await?;
//! // 首次同步成功后 status 变 success；窗口继续开着期间凭证变化
//! // 会自动重同步，用户可继续在浏览器里使用（如发消息过风控）
//! let st = mgr.status("doubao").await;
//! # Ok(())
//! # }
//! ```
//!
//! 平台 profile 目录按平台固定复用（`$TMPDIR/ai-web-provider-cdp/<platform>`）：
//! Chrome 的登录态留在 profile 里，下次 CDP 授权无需重新扫码；
//! 目录含登录凭证，安全性等价于 obscura storage 的 cookies.json。

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{mpsc, watch};

use crate::models::Command;

/// 轮询 CDP cookie 的间隔
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// 等待 Chrome 输出 DevTools WS 地址的超时
const CHROME_LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);
/// 授权窗口的总时长上限（登录 + 持续同步期；到点关闭浏览器，
/// 已同步的凭证不受影响）
const LOGIN_WAIT_TIMEOUT: Duration = Duration::from_secs(600);
/// ImportCookies（含页面刷新校验）的等待超时
const IMPORT_TIMEOUT: Duration = Duration::from_secs(120);
/// 持续同步的注入节流间隔：凭证指纹未变但上次注入失败（如 worker
/// 忙/页面抖动）时的重试间隔，防止 obscura 页面被反复刷新
const IMPORT_RETRY_THROTTLE: Duration = Duration::from_secs(8);

/// CDP 登录会话状态快照（供 HTTP 端点 / 前端轮询）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpLoginStatus {
    /// launching / waiting_login / collecting / importing / success / failed / cancelled
    pub phase: String,
    /// Chrome 进程 PID（launch 后有效）
    pub chrome_pid: Option<u32>,
    /// 会话已运行的秒数
    pub elapsed_secs: u64,
    /// 导入成功的 cookie 条数（success 时有效）
    pub imported: Option<usize>,
    /// 导入后的登录态校验结果（success 时有效）
    pub logged_in: Option<bool>,
    /// 人类可读的当前进展 / 错误详情
    pub message: String,
}

impl CdpLoginStatus {
    fn phase(phase: &str, message: impl Into<String>) -> Self {
        Self {
            phase: phase.to_string(),
            chrome_pid: None,
            elapsed_secs: 0,
            imported: None,
            logged_in: None,
            message: message.into(),
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(self.phase.as_str(), "success" | "failed" | "cancelled")
    }
}

/// 平台登录判定所依赖的关键 cookie 名（出现且非空即认为用户已登录）。
/// 未覆盖的平台返回空切片 → 只能靠 `finish()` 手动触发收集。
pub fn platform_login_cookie_names(platform: &str) -> &'static [&'static str] {
    match platform {
        // 豆包会话凭证（.doubao.com 域，HttpOnly）
        "doubao" => &["sessionid"],
        // ChatGPT 会话 token（约 30 天有效，活跃使用自动续期）
        "chatgpt" => &["__Secure-next-auth.session-token"],
        // Claude 会话凭证（HttpOnly，sk-ant-sid01- 前缀，.claude.ai 域）
        "claude" => &["sessionKey"],
        // DeepSeek 网页 API 鉴权 cookie
        "deepseek" => &["userToken"],
        // 千问：登录后 .qianwen.com 域的用户标识 cookie（SSO ticket
        // hash，qianwen-web 前端 USER_ID_COOKIE_KEY）；未出现时靠
        // CDP 向导的「我已完成登录」手动触发收集
        "qianwen" => &["tongyi_sso_ticket_hash"],
        // Kimi：moonshot passport 登录态（新旧版本 cookie 名并存）
        "kimi" => &["kimi_token", "kps", "kimi_mtoken"],
        _ => &[],
    }
}

/// 平台登录判定所依赖的关键 localStorage 键名。
/// DeepSeek 现版前端把会话凭证 `userToken` 写入 localStorage
/// （不再落 cookie），仅靠 cookie 检测不到登录——需轮询 localStorage。
/// 键值非空（profile 残留也算）即触发同步，有效性由 worker 导入校验
/// 判定；值变化（重新登录/轮换）触发重同步。
/// 空切片 = 该平台登录态与 localStorage 无关。
pub fn platform_login_storage_keys(platform: &str) -> &'static [&'static str] {
    match platform {
        "deepseek" => &["userToken"],
        _ => &[],
    }
}

/// 「聊天才会下发」的凭证 cookie 名：登录检测不覆盖它们，但用户在
/// 授权窗口里实际使用（发消息）后由服务端 Set-Cookie——值变化同样
/// 触发重同步，确保这类凭证不因窗口关闭而丢失。
pub fn platform_sync_cookie_names(platform: &str) -> &'static [&'static str] {
    match platform {
        // 阿里 baxia 风控凭证：向千问发送消息通过风控后下发，
        // 后续聊天请求携带它免滑块
        "qianwen" => &["x5sec"],
        _ => &[],
    }
}

/// 从 base_url 提取 security origin（scheme://host[:port]，去掉路径）。
/// 用于 CDP Storage.getDOMStorageItems 的 storageId。
fn origin_of(base_url: &str) -> String {
    let s = base_url.trim();
    let scheme_end = s.find("://").map(|i| i + 3).unwrap_or(0);
    let rest = &s[scheme_end..];
    let end = rest.find('/').unwrap_or(rest.len());
    s[..scheme_end + end].to_string()
}

/// 发现本机 Chrome / Chromium / Edge 可执行文件。
/// `CHROME_PATH`（或 `CHROME_BIN`）环境变量优先。
pub fn discover_chrome() -> Option<PathBuf> {
    for var in ["CHROME_PATH", "CHROME_BIN"] {
        if let Ok(p) = std::env::var(var) {
            if !p.is_empty() {
                let path = PathBuf::from(&p);
                if path.is_file() {
                    return Some(path);
                }
                tracing::warn!("{var}={p} 不存在，继续常规路径探测");
            }
        }
    }
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        ]
    } else if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ]
    } else {
        &[
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/microsoft-edge",
            "/snap/bin/chromium",
        ]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

/// 平台 CDP 授权 profile 目录（固定复用，Chrome 登录态留存其中）
pub(crate) fn profile_dir(platform: &str) -> PathBuf {
    std::env::temp_dir()
        .join("ai-web-provider-cdp")
        .join(platform)
}

/// 启动 Chrome（独立 profile + 随机调试端口），打开 target_url，
/// 从 stderr 解析 DevTools WS 地址。
/// 返回子进程与 `ws://127.0.0.1:<port>/devtools/browser/<uuid>`。
pub(crate) async fn launch_chrome(
    chrome: &PathBuf,
    target_url: &str,
    user_data_dir: &PathBuf,
) -> anyhow::Result<(Child, String)> {
    if let Err(e) = std::fs::create_dir_all(user_data_dir) {
        anyhow::bail!("创建 CDP profile 目录失败: {e}");
    }
    let mut child = TokioCommand::new(chrome)
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg("--remote-debugging-port=0")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--new-window")
        .arg(target_url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr = child.stderr.take().expect("stderr piped");
    let ws_url = tokio::time::timeout(CHROME_LAUNCH_TIMEOUT, async {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(addr) = line.split_once("DevTools listening on ") {
                return Some(addr.1.trim().to_string());
            }
        }
        None
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!("Chrome 启动超时（{CHROME_LAUNCH_TIMEOUT:?} 未输出 DevTools 地址）")
    })?
    .ok_or_else(|| {
        anyhow::anyhow!("Chrome 进程退出，未输出 DevTools 地址（端口被占用或实例已运行？")
    })?;

    Ok((child, ws_url))
}

/// 轻量 CDP WebSocket 客户端：只做 `id/method/params → result` 同步调用。
/// cdp_chat 模块复用（CDP 提问通道）
pub(crate) struct CdpClient {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
}

impl CdpClient {
    pub(crate) async fn connect(ws_url: &str) -> anyhow::Result<Self> {
        let (ws, _resp) = tokio_tungstenite::connect_async(ws_url).await?;
        Ok(Self { ws, next_id: 0 })
    }

    /// 调用 CDP 方法并等待同 id 应答（忽略事件通知）
    pub(crate) async fn call(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let req = json!({ "id": id, "method": method, "params": params });
        self.ws
            .send(tokio_tungstenite::tungstenite::Message::Text(
                req.to_string(),
            ))
            .await?;
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(30), self.ws.next())
                .await
                .map_err(|_| anyhow::anyhow!("CDP 应答超时（30s）"))?
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("CDP WebSocket 已断开"))?;
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                let v: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if v.get("id").and_then(Value::as_i64) == Some(id as i64) {
                    if let Some(err) = v.get("error") {
                        anyhow::bail!("CDP 错误: {err}");
                    }
                    return Ok(v["result"].clone());
                }
            }
        }
    }

    /// 收集全量 cookie（browser 级 Storage.getCookies，覆盖所有域）
    async fn get_all_cookies(&mut self) -> anyhow::Result<Vec<Value>> {
        let result = self.call("Storage.getCookies", json!({})).await?;
        Ok(result
            .get("cookies")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// 收集指定 origin 的 localStorage 全量条目
    /// （browser 级 Storage.getDOMStorageItems，无需 attach 页面）。
    async fn get_local_storage(
        &mut self,
        security_origin: &str,
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let result = self
            .call(
                "Storage.getDOMStorageItems",
                json!({ "storageId": {
                    "securityOrigin": security_origin,
                    "isLocalStorage": true,
                }}),
            )
            .await?;
        let entries = result
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = std::collections::HashMap::new();
        for entry in entries {
            let mut it = entry.as_array().map(|a| a.iter().cloned());
            let (k, v) = match (it.as_mut().and_then(|i| i.next()), it.and_then(|mut i| i.next())) {
                (Some(k), Some(v)) => (k, v),
                _ => continue,
            };
            let (Some(k), Some(v)) = (k.as_str(), v.as_str()) else {
                continue;
            };
            out.insert(k.to_string(), v.to_string());
        }
        Ok(out)
    }
}

/// CDP cookie（Storage.getCookies 条目）→ EditThisCookie JSON 条目。
/// 见 cookie_import::cookie_entry_to_set_cookie 支持的字段。
fn cdp_cookie_to_edit_this_cookie(c: &Value) -> Value {
    let domain = c.get("domain").and_then(Value::as_str).unwrap_or("");
    // CDP 的 domain 无前导点 = host-only cookie
    let host_only = !domain.starts_with('.');
    let expires = c
        .get("expires")
        .and_then(Value::as_f64)
        .filter(|e| *e > 0.0);
    // CDP: "Strict"/"Lax"/"None" → EditThisCookie: "strict"/"lax"/"no_restriction"
    // （cookie_entry_to_set_cookie 同时接受 "none" 与 "no_restriction"）
    let same_site = match c.get("sameSite").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s.to_ascii_lowercase(),
        _ => String::new(),
    };
    let mut out = json!({
        "name": c.get("name").and_then(Value::as_str).unwrap_or(""),
        "value": c.get("value").and_then(Value::as_str).unwrap_or(""),
        "domain": domain,
        "path": c.get("path").and_then(Value::as_str).unwrap_or("/"),
        "secure": c.get("secure").and_then(Value::as_bool).unwrap_or(false),
        "httpOnly": c.get("httpOnly").and_then(Value::as_bool).unwrap_or(false),
        "hostOnly": host_only,
        "sameSite": same_site,
    });
    if let Some(exp) = expires {
        out["expirationDate"] = json!(exp);
    }
    out
}

/// cookie 是否会被发送到 base_url 对应主机（RFC 6265 域匹配）。
/// `cookie_domain` 需已去掉前导点；host 等于 domain 或以 ".{domain}" 结尾即匹配。
fn cookie_matches_host(cookie_domain: &str, base_url: &str) -> bool {
    let host = base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("");
    if cookie_domain.is_empty() || host.is_empty() {
        return false;
    }
    host == cookie_domain || host.ends_with(&format!(".{cookie_domain}"))
}

/// 平台域关键登录 cookie 的 (name → value) 快照。
/// profile 固定复用会残留上一轮授权的（可能已过期的）关键 cookie，
/// "存在且非空"无法区分"新登录"与"残留旧值"——靠启动基线与轮询
/// 快照的值对比识别变化（新出现或值更新）。
fn login_cookie_snapshot(
    cookies: &[Value],
    names: &[&str],
    base_url: &str,
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for c in cookies {
        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
        if !names.contains(&name) {
            continue;
        }
        let domain = c
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_start_matches('.');
        if !cookie_matches_host(domain, base_url) {
            continue;
        }
        out.insert(
            name.to_string(),
            c.get("value").and_then(Value::as_str).unwrap_or("").to_string(),
        );
    }
    out
}

/// 凭证指纹：登录/聊天相关 cookie 与 localStorage 关键键的
/// 「名=值」排序拼接（空值跳过）。
/// 用途：① 非空即「可能已登录」信号（profile 残留也算——有效性由
/// worker 导入校验判定）；② 持续同步去重——指纹相对上次已同步的变化
/// 才重新注入，避免凭证未变时 obscura 页面被反复刷新。
fn credential_fingerprint(
    cookies: &[Value],
    cookie_names: &[&str],
    storage_keys: &[&str],
    storage: &std::collections::HashMap<String, String>,
    base_url: &str,
) -> String {
    let mut pairs: Vec<String> = Vec::new();
    for (name, value) in login_cookie_snapshot(cookies, cookie_names, base_url) {
        if !value.is_empty() {
            pairs.push(format!("{name}={value}"));
        }
    }
    for key in storage_keys {
        if let Some(v) = storage.get(*key) {
            if !v.is_empty() {
                pairs.push(format!("ls:{key}={v}"));
            }
        }
    }
    pairs.sort();
    pairs.join(";")
}

/// 按平台域过滤 + 转换为 EditThisCookie JSON 数组文本
/// （`Command::ImportCookies` 的输入格式之一）。
fn cdp_cookies_to_import_payload(cookies: &[Value], base_url: &str) -> String {    let filtered: Vec<Value> = cookies
        .iter()
        .filter(|c| {
            let domain = c
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_start_matches('.');
            cookie_matches_host(domain, base_url)
        })
        .map(cdp_cookie_to_edit_this_cookie)
        .collect();
    Value::Array(filtered).to_string()
}

// =============================================================================
// 会话管理
// =============================================================================

struct SessionInner {
    status: Mutex<CdpLoginStatus>,
    cancel: AtomicBool,
    /// 手动触发收集（finish()）信号
    finish_now: AtomicBool,
    /// 通知轮询任务立即退出并杀 Chrome
    kill_tx: mpsc::Sender<()>,
    /// 任务完成信号
    done_rx: watch::Receiver<bool>,
}

impl SessionInner {
    /// 终态写回状态（含 cancelled 竞态：cancel 先置 phase，这里不覆盖）
    fn set_terminal(
        &self,
        phase: &str,
        msg: String,
        imported: Option<usize>,
        logged_in: Option<bool>,
    ) {
        let mut st = self.status.lock().unwrap();
        if st.phase == "cancelled" {
            return;
        }
        st.phase = phase.to_string();
        st.message = msg;
        st.imported = imported;
        st.logged_in = logged_in;
    }

    /// 非终态 phase 写回（cancelled 竞态保护同上）
    fn set_phase(&self, phase: &str, msg: impl Into<String>) {
        let mut st = self.status.lock().unwrap();
        if st.phase == "cancelled" {
            return;
        }
        st.phase = phase.to_string();
        st.message = msg.into();
    }

    /// 仅更新 message（持续同步期间 phase 保持不变）
    fn set_message(&self, msg: impl Into<String>) {
        let mut st = self.status.lock().unwrap();
        if st.phase == "cancelled" {
            return;
        }
        st.message = msg.into();
    }
}

/// 单平台 CDP 登录会话句柄（克隆共享同一会话）
#[derive(Clone)]
pub struct CdpLoginSession {
    inner: Arc<SessionInner>,
}

impl CdpLoginSession {
    /// 当前状态快照
    pub async fn status(&self) -> CdpLoginStatus {
        self.inner.status.lock().unwrap().clone()
    }

    /// 取消授权：杀 Chrome、终止轮询。
    /// failed/cancelled（浏览器已关）无效果；success（凭证已同步、浏览器
    /// 仍开着）时同样关闭浏览器结束会话。
    pub async fn cancel(&self) {
        // 任务已收尾（浏览器已被关闭/超时清理），无可取消
        if *self.inner.done_rx.borrow() {
            return;
        }
        let phase = { self.inner.status.lock().unwrap().phase.clone() };
        if phase == "failed" || phase == "cancelled" {
            return;
        }
        self.inner.status.lock().unwrap().phase = "cancelled".into();
        self.inner.cancel.store(true, Ordering::SeqCst);
        let _ = self.inner.kill_tx.send(()).await;
        // 等任务收尾（杀 Chrome）；clone 出可变 receiver
        let mut rx = self.inner.done_rx.clone();
        let _ = rx.changed().await;
    }

    /// 手动触发 cookie 收集（用户确认自己已登录，不等自动检测）。
    /// 未启动 / 已结束的会话返回 false。
    pub fn finish(&self) -> bool {
        let phase = { self.inner.status.lock().unwrap().phase.clone() };
        if phase != "waiting_login" {
            return false;
        }
        self.inner.finish_now.store(true, Ordering::SeqCst);
        true
    }
}

/// 各平台 CDP 登录会话管理器。
/// 同一时间每平台只允许一个活跃会话；重复 start 会拒绝并提示。
pub struct CdpLoginManager {
    sessions: Mutex<HashMap<String, CdpLoginSession>>,
}

impl Default for CdpLoginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CdpLoginManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// 拉起 Chrome 开始 CDP 授权。
    ///
    /// - `platform`：平台标识（doubao / chatgpt / ...）
    /// - `base_url`：平台站点地址（Chrome 打开它；也用于 cookie 域过滤）
    /// - `worker_tx`：该平台 obscura worker 的命令通道（ImportCookies 注入用）
    pub async fn start(
        &self,
        platform: &str,
        base_url: &str,
        worker_tx: mpsc::Sender<Command>,
    ) -> anyhow::Result<()> {
        // 存在性/终态检查：同步完成，guard 不跨 await——
        // 保持 start future 为 Send（网关 ask 失败自动重授权用
        // tokio::spawn 调用；actix handler 直接 await 不要求 Send，
        // 此前 guard 跨 await 未暴露）
        {
            let sessions = self.sessions.lock().unwrap();
            let busy = sessions
                .get(platform)
                .map(|s| !s.inner.status.lock().unwrap().is_terminal())
                .unwrap_or(false);
            if busy {
                anyhow::bail!("平台 {platform} 已有进行中的 CDP 授权会话");
            }
        }

        let chrome = discover_chrome().ok_or_else(|| {
            anyhow::anyhow!("未找到 Chrome/Chromium/Edge，请安装或设置 CHROME_PATH 环境变量")
        })?;

        let (mut child, ws_url) = launch_chrome(&chrome, base_url, &profile_dir(platform)).await?;
        let chrome_pid = child.id();
        let ws_url_owned = ws_url.clone();
        let platform_owned = platform.to_string();
        let base_url_owned = base_url.to_string();

        let (kill_tx, mut kill_rx) = mpsc::channel::<()>(1);
        let (done_tx, done_rx) = watch::channel(false);

        let session = CdpLoginSession {
            inner: Arc::new(SessionInner {
                status: Mutex::new(CdpLoginStatus::phase(
                    "waiting_login",
                    "Chrome 已启动，请在浏览器中完成登录",
                )),
                cancel: AtomicBool::new(false),
                finish_now: AtomicBool::new(false),
                kill_tx: kill_tx.clone(),
                done_rx,
            }),
        };
        self.sessions
            .lock()
            .unwrap()
            .insert(platform.to_string(), session.clone());

        let inner = session.inner.clone();

        // 轮询任务：持续同步凭证——检测登录/聊天相关键值变化 → 收集 →
        // ImportCookies 注入 → 首次校验通过置 success（窗口保持开启继续同步）
        let poll_task = tokio::spawn(async move {
            let started = Instant::now();
            let login_names = platform_login_cookie_names(&platform_owned);
            // localStorage 登录检测（DeepSeek：userToken 在 localStorage 不落 cookie）
            let storage_keys = platform_login_storage_keys(&platform_owned);
            // 聊天才会下发的凭证 cookie（千问 x5sec）——同样计入指纹
            let sync_names = platform_sync_cookie_names(&platform_owned);
            let mut cookie_names: Vec<&str> = login_names.to_vec();
            cookie_names.extend_from_slice(sync_names);
            let storage_origin = origin_of(&base_url_owned);
            // 已同步的凭证指纹与最近一次注入尝试时间（失败重试节流）
            let mut last_synced: Option<String> = None;
            let mut last_attempt: Option<Instant> = None;
            // 首次同步成功后窗口保持开启，凭证变化持续重同步
            let mut ever_success = false;

            'outer: loop {
                // 状态心跳（elapsed / pid）
                {
                    let mut st = inner.status.lock().unwrap();
                    st.elapsed_secs = started.elapsed().as_secs();
                    if st.chrome_pid.is_none() {
                        st.chrome_pid = chrome_pid;
                    }
                }

                tokio::select! {
                    _ = kill_rx.recv() => break 'outer,
                    _ = tokio::time::sleep(POLL_INTERVAL) => {}
                }

                if inner.cancel.load(Ordering::SeqCst) {
                    break 'outer;
                }

                // 连接（重）建立
                let mut client = match CdpClient::connect(&ws_url_owned).await {
                    Ok(c) => c,
                    Err(e) => {
                        // Chrome 被用户手动关闭：已同步过凭证则保持成功终态
                        if let Ok(Some(_)) = child.try_wait() {
                            if ever_success {
                                let (imp, logged) = {
                                    let st = inner.status.lock().unwrap();
                                    (st.imported, st.logged_in)
                                };
                                inner.set_terminal(
                                    "success",
                                    "Chrome 已关闭，本次授权完成（凭证均已同步）".into(),
                                    imp,
                                    logged,
                                );
                            } else {
                                inner.set_terminal(
                                    "failed",
                                    "Chrome 已退出（用户关闭？），授权中止".into(),
                                    None,
                                    None,
                                );
                            }
                            break 'outer;
                        }
                        tracing::warn!("[cdp:{}] CDP 连接失败，重试: {e}", platform_owned);
                        continue;
                    }
                };

                // 轮询循环（复用同一连接）
                loop {
                    if inner.cancel.load(Ordering::SeqCst) {
                        break 'outer;
                    }
                    if started.elapsed() > LOGIN_WAIT_TIMEOUT {
                        inner.set_terminal(
                            "failed",
                            format!("等待登录超时（{} 分钟）", LOGIN_WAIT_TIMEOUT.as_secs() / 60),
                            None,
                            None,
                        );
                        break 'outer;
                    }
                    let cookies = match client.get_all_cookies().await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("[cdp:{}] 获取 cookie 失败，重连: {e}", platform_owned);
                            break; // 内层循环 → 外层重连
                        }
                    };
                    // localStorage 快照（失败/不支持时退化为空表，仅影响自动检测灵敏度）
                    let storage_now = if storage_keys.is_empty() {
                        std::collections::HashMap::new()
                    } else {
                        client.get_local_storage(&storage_origin).await.unwrap_or_default()
                    };
                    // 基线快照（重连后保留，Chrome 未重启 cookie 未变）：
                    // profile 残留的旧 token 不算"新登录"信号
                    if baseline.is_none() {
                        let snap = login_cookie_snapshot(&cookies, &login_names, &base_url_owned);
                        let stale: Vec<&str> = snap
                            .iter()
                            .filter(|(_, v)| !v.is_empty())
                            .map(|(k, _)| k.as_str())
                            .collect();
                        if !stale.is_empty() {
                            let mut st = inner.status.lock().unwrap();
                            st.message =
                                "检测到 profile 残留登录态，请在 Chrome 中重新登录（或完成后手动确认）"
                                    .into();
                        }
                        baseline = Some(snap);
                    }
                    // localStorage 基线（与 cookie 基线同语义：残留旧值不算新登录）
                    if storage_baseline.is_none() && !storage_keys.is_empty() {
                        let snap: std::collections::HashMap<String, String> = storage_keys
                            .iter()
                            .filter_map(|k| {
                                storage_now
                                    .get(*k)
                                    .map(|v| (k.to_string(), v.clone()))
                            })
                            .collect();
                        storage_baseline = Some(snap);
                    }
                    // 自动检测：关键 cookie 出现且非空，且值相对基线有变化
                    //（新出现或更新——扫码登录会写入新 token；残留旧值不触发）
                    let current = login_cookie_snapshot(&cookies, &login_names, &base_url_owned);
                    let cookie_logged = !login_names.is_empty()
                        && current.iter().any(|(name, value)| {
                            !value.is_empty()
                                && baseline
                                    .as_ref()
                                    .and_then(|b| b.get(name))
                                    .map(|old| old != value)
                                    .unwrap_or(true)
                        });
                    // localStorage 检测同语义：关键键非空且相对基线有变化
                    let storage_logged = storage_keys.iter().any(|k| {
                        let Some(v) = storage_now.get(*k) else {
                            return false;
                        };
                        !v.is_empty()
                            && storage_baseline
                                .as_ref()
                                .and_then(|b| b.get(*k))
                                .map(|old| old != v)
                                .unwrap_or(true)
                    });
                    // 残留 storage 凭证自动收集（DeepSeek）：基线即有非空关键键
                    // 且值一直未变。前端保留它说明仍有效（失效 token 会被前端
                    // 在初始化时移除）——持续超过宽限期即自动触发收集，免去
                    // 用户手动确认；值被清除（失效）或变化（重新登录）则重置
                    // 计时，转回变化检测路径
                    let mut residual_ready = false;
                    if !storage_keys.is_empty() {
                        if let Some(b) = storage_baseline.as_ref() {
                            if storage_is_residual(storage_keys, &storage_now, b) {
                                match residual_since {
                                    None => {
                                        residual_since = Some(Instant::now());
                                        let mut st = inner.status.lock().unwrap();
                                        if st.phase == "waiting_login" {
                                            st.message =
                                                "检测到浏览器残留登录凭证，正在确认其有效性…"
                                                    .into();
                                        }
                                    }
                                    Some(t) => {
                                        if t.elapsed() >= RESIDUAL_STORAGE_GRACE {
                                            residual_ready = true;
                                        }
                                    }
                                }
                            } else if residual_since.take().is_some() {
                                // 残留凭证被清除/轮换：恢复默认提示
                                let mut st = inner.status.lock().unwrap();
                                if st.phase == "waiting_login" {
                                    st.message = "Chrome 已启动，请在浏览器中完成登录".into();
                                }
                            }
                        }
                    }
                    let logged = cookie_logged || storage_logged || residual_ready;
                    let manual = inner.finish_now.load(Ordering::SeqCst);
                    if logged || manual {
                        // 收集 → 注入
                        {
                            let mut st = inner.status.lock().unwrap();
                            st.phase = "importing".into();
                            st.message = "检测到登录态，正在收集 cookie 并注入引擎".into();
                        }
                        let payload = cdp_cookies_to_import_payload(&cookies, &base_url_owned);
                        // localStorage 一并收集（DeepSeek 的 userToken 在这里；
                        // storage_now 是本轮完整快照，非空才传，避免空表噪音）
                        let local_storage = if storage_keys.is_empty() || storage_now.is_empty() {
                            None
                        } else {
                            Some(storage_now.clone())
                        };
                        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                        if worker_tx
                            .send(Command::ImportCookies {
                                cookies: payload,
                                local_storage,
                                reply_tx,
                            })
                            .await
                            .is_err()
                        {
                            inner.set_terminal("failed", "平台 worker 已停止".into(), None, None);
                            break 'outer;
                        }
                        match tokio::time::timeout(IMPORT_TIMEOUT, reply_rx).await {
                            Ok(Ok(Ok(resp))) => {
                                inner.set_terminal(
                                    "success",
                                    resp.message.clone(),
                                    Some(resp.imported),
                                    Some(resp.logged_in),
                                );
                            }
                            Ok(Ok(Err(e))) => {
                                inner.set_terminal(
                                    "failed",
                                    format!("cookie 注入失败: {e}"),
                                    None,
                                    None,
                                );
                            }
                            Ok(Err(_)) => {
                                inner.set_terminal(
                                    "failed",
                                    "worker 未响应 cookie 导入".into(),
                                    None,
                                    None,
                                );
                            }
                            Err(_) => {
                                inner.set_terminal(
                                    "failed",
                                    format!("cookie 导入超时（{}s）", IMPORT_TIMEOUT.as_secs()),
                                    None,
                                    None,
                                );
                            }
                        }
                        break 'outer;
                    }
                    tokio::select! {
                        _ = kill_rx.recv() => break 'outer,
                        _ = tokio::time::sleep(POLL_INTERVAL) => {}
                    }
                }
            }

            // 收尾：杀 Chrome + done 信号
            let _ = child.kill().await;
            let _ = done_tx.send(true);
        });

        // 保持会话引用（任务结束也不移除——status 查询终态；下次 start 覆盖）
        let _ = poll_task;
        let _ = kill_tx;
        Ok(())
    }

    /// 查询平台会话状态；无会话返回 None
    pub async fn status(&self, platform: &str) -> Option<CdpLoginStatus> {
        let sessions = self.sessions.lock().unwrap();
        match sessions.get(platform) {
            Some(s) => Some(s.status().await),
            None => None,
        }
    }

    /// 取消平台会话；无会话返回 false
    pub async fn cancel(&self, platform: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        match sessions.get(platform) {
            Some(s) => {
                s.cancel().await;
                true
            }
            None => false,
        }
    }

    /// 手动触发收集（用户已确认登录）；无进行中的会话返回 false
    pub async fn finish(&self, platform: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        sessions.get(platform).map(|s| s.finish()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdp_cookie_conversion() {
        let cdp = json!({
            "domain": ".doubao.com", "expires": 4102444800.0, "httpOnly": true,
            "name": "sessionid", "path": "/", "sameSite": "Lax", "secure": true, "value": "abc"
        });
        let out = cdp_cookie_to_edit_this_cookie(&cdp);
        assert_eq!(out["name"], "sessionid");
        assert_eq!(out["domain"], ".doubao.com");
        assert_eq!(out["hostOnly"], false);
        assert_eq!(out["sameSite"], "lax");
        assert_eq!(out["expirationDate"], 4102444800.0);

        // host-only（无前导点）+ session cookie（无 expires）
        let cdp2 = json!({
            "domain": "www.doubao.com", "httpOnly": false, "name": "sid",
            "path": "/", "sameSite": "None", "secure": true, "value": "x"
        });
        let out2 = cdp_cookie_to_edit_this_cookie(&cdp2);
        assert_eq!(out2["hostOnly"], true);
        assert_eq!(out2["sameSite"], "none");
        assert!(out2.get("expirationDate").is_none());
    }

    #[test]
    fn host_matching() {
        assert!(cookie_matches_host(
            "doubao.com",
            "https://www.doubao.com/chat/"
        ));
        assert!(cookie_matches_host("doubao.com", "https://doubao.com"));
        assert!(!cookie_matches_host("doubao.com", "https://chatgpt.com"));
        assert!(!cookie_matches_host("", "https://www.doubao.com"));
        // 子域 cookie 不发送到父域以外的兄弟域
        assert!(!cookie_matches_host(
            "passport.douyin.com",
            "https://www.doubao.com"
        ));
    }

    #[test]
    fn payload_filters_by_domain() {
        let cookies = vec![
            json!({"domain": ".doubao.com", "name": "sessionid", "value": "v1", "path": "/"}),
            json!({"domain": ".chatgpt.com", "name": "other", "value": "v2", "path": "/"}),
            json!({"domain": "www.doubao.com", "name": "sid", "value": "v3", "path": "/"}),
        ];
        let payload = cdp_cookies_to_import_payload(&cookies, "https://www.doubao.com/chat/");
        let arr: Value = serde_json::from_str(&payload).unwrap();
        let names: Vec<&str> = arr
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["sessionid", "sid"]);
    }

    #[test]
    fn login_cookie_names_covered() {
        assert_eq!(platform_login_cookie_names("doubao"), &["sessionid"]);
        assert!(!platform_login_cookie_names("unknown").is_empty() == false);
    }

    #[test]
    fn snapshot_and_change_detection() {
        let names = ["userToken"];
        let base = "https://chat.deepseek.com";
        // 基线：profile 残留的旧 token
        let cookies = vec![
            json!({"domain": "chat.deepseek.com", "name": "userToken", "value": "stale", "path": "/"}),
            json!({"domain": "chat.deepseek.com", "name": "other", "value": "x", "path": "/"}),
            // 异域同名 cookie 不得进快照
            json!({"domain": "evil.example.com", "name": "userToken", "value": "evil", "path": "/"}),
        ];
        let baseline = login_cookie_snapshot(&cookies, &names, base);
        assert_eq!(baseline.get("userToken").map(String::as_str), Some("stale"));

        // 残留旧值不变 → 不触发（logged=false）
        let cur = login_cookie_snapshot(&cookies, &names, base);
        let logged = cur.iter().any(|(n, v)| {
            !v.is_empty()
                && baseline.get(n).map(|old| old != v).unwrap_or(true)
        });
        assert!(!logged, "残留旧值不得触发新登录判定");

        // 值更新（扫码后新 token）→ 触发
        let mut updated = cookies.clone();
        updated[0]["value"] = json!("fresh");
        let cur2 = login_cookie_snapshot(&updated, &names, base);
        let logged2 = cur2.iter().any(|(n, v)| {
            !v.is_empty()
                && baseline.get(n).map(|old| old != v).unwrap_or(true)
        });
        assert!(logged2, "新 token 值必须触发收集");

        // 空值 → 不触发
        updated[0]["value"] = json!("");
        let cur3 = login_cookie_snapshot(&updated, &names, base);
        let logged3 = cur3.iter().any(|(n, v)| {
            !v.is_empty()
                && baseline.get(n).map(|old| old != v).unwrap_or(true)
        });
        assert!(!logged3);

        // 干净 profile（基线无 token）→ 新出现即触发
        let empty: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let logged4 = cur2.iter().any(|(n, v)| {
            !v.is_empty() && empty.get(n).map(|old| old != v).unwrap_or(true)
        });
        assert!(logged4);
    }

    /// 残留 storage 凭证判定（DeepSeek userToken 场景）
    #[test]
    fn residual_storage_detection() {
        let keys = ["userToken"];
        let mut now = std::collections::HashMap::new();
        let mut baseline = std::collections::HashMap::new();
        baseline.insert("userToken".to_string(), "tok".to_string());

        // 残留：值与基线相同且非空 → 进入宽限期观察
        now.insert("userToken".to_string(), "tok".to_string());
        assert!(storage_is_residual(&keys, &now, &baseline));

        // 值变化（重新登录 / token 轮换）→ 非残留，走变化检测立即触发
        now.insert("userToken".to_string(), "fresh".to_string());
        assert!(!storage_is_residual(&keys, &now, &baseline));

        // 值被清除（前端移除失效 token）→ 非残留，转为等待新登录
        now.remove("userToken");
        assert!(!storage_is_residual(&keys, &now, &baseline));

        // 空字符串视为清除
        now.insert("userToken".to_string(), String::new());
        assert!(!storage_is_residual(&keys, &now, &baseline));

        // 干净 profile（基线为空）→ 新出现的 token 不算残留
        let empty = std::collections::HashMap::new();
        now.insert("userToken".to_string(), "tok".to_string());
        assert!(!storage_is_residual(&keys, &now, &empty));
    }
}
