//! CDP Chrome 提问通道。
//!
//! 千问 baxia 风控对 obscura 无头环境强制滑块验证（环境指纹判定，
//! cookie 转移无效、滑块组件加载超时无解），真 Chrome 不触发。
//! 本模块复用 CDP 授权的持久 profile（登录态与有效 x5sec 留存其中），
//! 拉起可见的 Chrome 窗口完成提问；若仍偶发风控，用户可在窗口中
//! 手动完成滑块，轮询检测到弹窗消失后自动继续。

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::process::Child;

use crate::cdp_auth::{discover_chrome, launch_chrome, profile_dir, CdpClient};

/// 页面 target 出现在 /json/list 的等待上限
const PAGE_TARGET_TIMEOUT: Duration = Duration::from_secs(15);
/// /json/list 轮询间隔
const PAGE_TARGET_POLL_MS: u64 = 300;

/// CDP Chrome 提问会话：持有 Chrome 子进程与页面级 WS 连接。
/// Chrome 窗口对用户可见；会话可跨多次 ask 复用。
pub struct CdpChatSession {
    child: Child,
    page: CdpClient,
}

impl CdpChatSession {
    /// 启动 Chrome（复用授权 profile）并连接到首个页面 target。
    ///
    /// 失败常见原因：profile 被一个已打开的授权 Chrome 占用
    /// （新进程会把启动委托给旧实例后退出，读不到 DevTools 地址）。
    pub async fn launch(platform: &str, target_url: &str) -> Result<Self> {
        let chrome = discover_chrome()
            .ok_or_else(|| anyhow!("未找到 Chrome，请先安装 Chrome 或改用扫码登录"))?;
        let profile = profile_dir(platform);
        let (child, browser_ws) = launch_chrome(&chrome, target_url, &profile)
            .await
            .map_err(|e| {
                anyhow!("启动 Chrome 失败（若已打开授权 Chrome 窗口，请先关闭它再提问）: {e}")
            })?;

        // ws://127.0.0.1:<port>/devtools/browser/<uuid> → 提取 HTTP 调试端口
        let port = parse_devtools_port(&browser_ws)?;
        let page_ws = wait_page_ws_url(port).await?;
        let page = CdpClient::connect(&page_ws)
            .await
            .map_err(|e| anyhow!("CDP 连接失败（连接页面调试端口）: {e}"))?;
        Ok(Self { child, page })
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

    /// Chrome 子进程是否仍在运行
    pub async fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// 关闭 Chrome
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
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

/// 轮询 `http://127.0.0.1:<port>/json/list` 直到出现页面 target，
/// 返回其页面级 webSocketDebuggerUrl。
async fn wait_page_ws_url(port: u16) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let list_url = format!("http://127.0.0.1:{port}/json/list");
    let deadline = Instant::now() + PAGE_TARGET_TIMEOUT;
    loop {
        if let Ok(resp) = client.get(&list_url).send().await {
            if let Ok(body) = resp.text().await {
                if let Ok(targets) = serde_json::from_str::<Vec<Value>>(&body) {
                    // 优先取 type=page 的 target（Chrome 启动只带一个页面）
                    if let Some(ws) = targets
                        .iter()
                        .find(|t| t.get("type").and_then(Value::as_str) == Some("page"))
                        .and_then(|t| t.get("webSocketDebuggerUrl"))
                        .and_then(Value::as_str)
                    {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        if Instant::now() > deadline {
            anyhow::bail!("{PAGE_TARGET_TIMEOUT:?} 内未在 /json/list 发现页面 target");
        }
        tokio::time::sleep(Duration::from_millis(PAGE_TARGET_POLL_MS)).await;
    }
}
