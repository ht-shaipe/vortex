//! CDP Chrome 提问通道。
//!
//! 千问 baxia 风控对 obscura 无头环境强制滑块验证（环境指纹判定，
//! cookie 转移无效、滑块组件加载超时无解），真 Chrome 不触发。
//! 本模块复用 CDP 授权的持久 profile（登录态与有效 x5sec 留存其中），
//! 拉起可见的 Chrome 窗口完成提问；若仍偶发风控，用户可在窗口中
//! 手动完成滑块，轮询检测到弹窗消失后自动继续。
//!
//! profile 可能正被一个**活着的** CDP 授权 Chrome 占用（授权成功后
//! 窗口保持开启持续同步凭证）——此时必须复用该实例而不是再拉一个：
//! profile 锁被占时新 Chrome 会把打开请求转发给旧实例后直接退出。
//! 探测失败（孤儿已死）才清理残留并启动新实例。

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Child;

use crate::cdp_auth::{
    discover_chrome, kill_stale_profile_chrome, launch_chrome, profile_dir, CdpClient,
};

/// 页面 target 出现在 /json/list 的等待上限
const PAGE_TARGET_TIMEOUT: Duration = Duration::from_secs(15);
/// /json/list 轮询间隔
const PAGE_TARGET_POLL_MS: u64 = 300;
/// 孤儿 Chrome 退出（SIGTERM）后等待 profile 锁释放的时间
const STALE_KILL_WAIT: Duration = Duration::from_millis(800);

/// CDP Chrome 提问会话：持有 Chrome 子进程与页面级 WS 连接。
/// Chrome 窗口对用户可见；会话可跨多次 ask 复用。
pub struct CdpChatSession {
    /// 自启动的 Chrome 子进程；复用已运行实例（授权窗口）时为 None
    child: Option<Child>,
    page: CdpClient,
}

impl CdpChatSession {
    /// 启动 Chrome（复用授权 profile）并连接到平台页面 target。
    ///
    /// 优先复用已运行的 CDP Chrome（授权窗口保持开启时 profile 被其
    /// 占用，新启动的实例只会把 URL 转发给它后退出）；无活实例时清理
    /// 占锁的孤儿再启动新的。
    pub async fn launch(platform: &str, target_url: &str) -> Result<Self> {
        let profile = profile_dir(platform);
        // 1) 探测已运行实例（授权窗口/上轮会话存活的 Chrome）
        if let Some(session) = try_attach_running(&profile, target_url).await? {
            tracing::info!("[cdp-chat:{platform}] 复用已运行的 CDP Chrome");
            return Ok(session);
        }
        // 2) 无活实例：清理可能占锁的孤儿后启动新的
        let chrome = discover_chrome()
            .ok_or_else(|| anyhow!("未找到 Chrome，请先安装 Chrome 或改用扫码登录"))?;
        kill_stale_profile_chrome(&profile);
        tokio::time::sleep(STALE_KILL_WAIT).await;
        let (child, browser_ws) = launch_chrome(&chrome, target_url, &profile)
            .await
            .map_err(|e| {
                anyhow!("启动 Chrome 失败（若已打开授权 Chrome 窗口，请先关闭它再提问）: {e}")
            })?;

        // ws://127.0.0.1:<port>/devtools/browser/<uuid> → 提取 HTTP 调试端口
        let port = parse_devtools_port(&browser_ws)?;
        let page_ws = wait_page_ws_url(port, target_url).await?;
        let page = CdpClient::connect(&page_ws)
            .await
            .map_err(|e| anyhow!("CDP 连接失败（连接页面调试端口）: {e}"))?;
        Ok(Self {
            child: Some(child),
            page,
        })
    }

    /// 在页面上下文执行 JS 并返回值（returnByValue + awaitPromise）。
    /// JS 抛错时返回 Err（含异常详情）。
    pub async fn evaluate(&mut self, js: &str) -> Result<Value> {
        let result = self
            .page
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": js,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
            )
            .await
            .map_err(|e| {
                anyhow!("CDP 连接失败（Chrome 窗口被关闭或调试连接中断）: {e}")
            })?;
        if let Some(details) = result.get("exceptionDetails") {
            let desc = details
                .pointer("/exception/description")
                .and_then(Value::as_str)
                .or_else(|| details.get("text").and_then(Value::as_str))
                .unwrap_or("未知 JS 异常");
            return Err(anyhow!("页面 JS 异常: {desc}"));
        }
        Ok(result
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Chrome 子进程是否仍在运行（复用实例时探测页面连接）
    pub async fn alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => {
                self.page
                    .call("Runtime.evaluate", json!({"expression": "1"}))
                    .await
                    .is_ok()
            }
        }
    }

    /// 关闭 Chrome（复用的授权窗口不归本会话关闭）
    pub async fn shutdown(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill().await;
        }
        self.child = None;
    }
}

/// 从 browser 级 WS 地址解析调试端口
fn parse_devtools_port(browser_ws: &str) -> Result<u16> {
    // 去掉协议前缀后取 host:port，再按 ':' 分割
    let rest = browser_ws
        .trim_start_matches("ws://")
        .trim_start_matches("http://");
    let port = rest
        .split('/')
        .next()
        .and_then(|hp| hp.rsplit_once(':'))
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .ok_or_else(|| anyhow!("无法从 DevTools 地址解析端口: {browser_ws}"))?;
    Ok(port)
}

/// 探测 profile 对应的 DevTools HTTP 调试端口（DevToolsActivePort 文件）。
/// 端口不存活（文件缺失/实例已死）返回 None。
async fn probe_devtools_port(profile: &PathBuf) -> Option<u16> {
    let text = std::fs::read_to_string(profile.join("DevToolsActivePort")).ok()?;
    let port = text.lines().next()?.trim().parse::<u16>().ok()?;
    // 探活：/json/version 可达才算活实例（文件可能是死实例的残留）
    let url = format!("http://127.0.0.1:{port}/json/version");
    let resp = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?
        .get(&url)
        .send()
        .await
        .ok()?;
    if resp.status().is_success() {
        Some(port)
    } else {
        None
    }
}

/// 尝试连接已运行 CDP Chrome 的平台页面（复用授权窗口）。
/// 无活实例 / 无平台页面时返回 Ok(None)。
async fn try_attach_running(profile: &PathBuf, target_url: &str) -> Result<Option<CdpChatSession>> {
    let Some(port) = probe_devtools_port(profile).await else {
        return Ok(None);
    };
    let origin = origin_prefix_of(target_url);
    let Some(page_ws) = find_page_ws_url(port, &origin).await? else {
        return Ok(None);
    };
    let page = CdpClient::connect(&page_ws)
        .await
        .map_err(|e| anyhow!("CDP 连接失败（连接已运行 Chrome 的页面）: {e}"))?;
    Ok(Some(CdpChatSession { child: None, page }))
}

/// 从站点 URL 提取 origin 前缀（scheme://host[:port]），用于匹配页面 target。
fn origin_prefix_of(target_url: &str) -> String {
    let s = target_url.trim();
    let scheme_end = s.find("://").map(|i| i + 3).unwrap_or(0);
    let rest = &s[scheme_end..];
    let end = rest.find('/').unwrap_or(rest.len());
    s[..scheme_end + end].to_string()
}

/// 在 /json/list 中找平台页面 target 的 webSocketDebuggerUrl。
/// 找不到匹配页面返回 Ok(None)。
async fn find_page_ws_url(port: u16, origin: &str) -> Result<Option<String>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let list_url = format!("http://127.0.0.1:{port}/json/list");
    let body = client.get(&list_url).send().await?.text().await?;
    let targets: Vec<Value> = serde_json::from_str(&body)?;
    Ok(targets
        .iter()
        .find(|t| {
            t.get("type").and_then(Value::as_str) == Some("page")
                && t
                    .get("url")
                    .and_then(Value::as_str)
                    .map(|u| u.starts_with(origin))
                    .unwrap_or(false)
        })
        .and_then(|t| t.get("webSocketDebuggerUrl"))
        .and_then(Value::as_str)
        .map(String::from))
}

/// 轮询 `http://127.0.0.1:<port>/json/list` 直到出现平台页面 target，
/// 返回其页面级 webSocketDebuggerUrl。
async fn wait_page_ws_url(port: u16, target_url: &str) -> Result<String> {
    let origin = origin_prefix_of(target_url);
    let deadline = Instant::now() + PAGE_TARGET_TIMEOUT;
    loop {
        if let Some(ws) = find_page_ws_url(port, &origin).await? {
            return Ok(ws);
        }
        if Instant::now() > deadline {
            anyhow::bail!("{PAGE_TARGET_TIMEOUT:?} 内未在 /json/list 发现平台页面 target");
        }
        tokio::time::sleep(Duration::from_millis(PAGE_TARGET_POLL_MS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_port_parsing() {
        assert_eq!(
            parse_devtools_port("ws://127.0.0.1:61928/devtools/browser/uuid-x").unwrap(),
            61928
        );
        assert!(parse_devtools_port("ws://bad/devtools/browser/uuid").is_err());
    }

    #[test]
    fn origin_prefix() {
        assert_eq!(
            origin_prefix_of("https://www.qianwen.com/chat"),
            "https://www.qianwen.com"
        );
        assert_eq!(
            origin_prefix_of("https://chat.deepseek.com/"),
            "https://chat.deepseek.com"
        );
    }
}
