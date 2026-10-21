//! 命令与数据模型。
//!
//! 宿主（vortex 网关）通过 [`Command`] 向已启动的平台 worker 发送任务；
//! worker 通过 oneshot 通道回传 [`AiReply`] 等结果。HTTP 层复用本模块的
//! 请求/响应结构（`AskRequest`、`StatusResponse`、`QrResponse` 等）。

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

/// ask 失败且根因为"登录态失效"时的稳定错误前缀。
/// 宿主（vortex 网关）据此识别并自动拉起 CDP 重授权
/// （拉起本地 Chrome → 用户登录 → 凭证自动注入 worker），
/// 剥离前缀后不影响人类可读的指引文案。
pub const ERR_NOT_LOGGED_IN: &str = "[auth-required]";

/// POST /api/doubao/ask 请求体
#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub question: String,
    /// 为 true 时以 SSE（text/event-stream）逐块返回豆包流式回复；
    /// 省略或为 false 时阻塞，等答完一次性返回完整 reply。
    #[serde(default)]
    pub stream: Option<bool>,
}

/// POST /api/{platform}/ask 的响应体。
#[derive(Debug, Serialize)]
pub struct AskResponse {
    /// 最终答案文本
    pub reply: String,
    /// 深度思考模型的思考过程（未触发深度思考时为空字符串）
    pub thinking: String,
    /// 联网搜索使用的关键字（未触发搜索时为空数组）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub search_queries: Vec<String>,
    /// 联网搜索/引用的参考资料（无搜索时为空数组）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Reference>,
}

/// 参考资料条目（联网搜索来源）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reference {
    /// 来源标题
    pub title: String,
    /// 来源链接
    pub url: String,
}

/// AI 回复内容：思考过程与最终答案分离
#[derive(Debug, Clone, Default, Serialize)]
pub struct AiReply {
    /// 深度思考过程（豆包思考块，非 10000 文本块）；未触发深度思考时为空
    pub thinking: String,
    /// 最终答案文本（block_type=10000 的文本块）
    pub reply: String,
    /// 联网搜索关键字（block_type=10025 块 queries 字段）
    #[serde(default)]
    pub search_queries: Vec<String>,
    /// 联网搜索参考资料（block_type=10025 搜索结果块中的来源列表）
    #[serde(default)]
    pub references: Vec<Reference>,
}

/// `GET /api/{platform}/status` 的响应体。
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    /// 当前是否处于登录状态
    pub logged_in: bool,
    /// 登录弹窗二维码状态（logged_in=true 时恒为 "none"）：
    /// - "ready"：弹窗开着，二维码等待扫码
    /// - "scanned"：已扫码，等手机上确认
    /// - "expired"：二维码已失效（页面显示「二维码失效/点击刷新」），重新 login 获取新码
    /// - "none"：当前页面没有登录弹窗
    pub qr_state: String,
}

/// 登录二维码响应。
#[derive(Debug, Serialize)]
pub struct QrResponse {
    /// 登录二维码：data: URL 或可直接 <img src="..."> 的地址
    pub qr_data_url: String,
    /// 二维码已保存的本地图片文件路径（当前目录 {platform}_login_qr.svg/.png）；
    /// 保存失败或二维码为外链时缺省
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_file: Option<String>,
    /// 给用户的提示文案（扫码/确认步骤说明）
    pub tip: String,
}

/// POST /api/{platform}/login/phone 请求体：手机号登录。
#[derive(Debug, Deserialize)]
pub struct LoginPhoneRequest {
    /// 手机号（含国家码与否由平台页面决定）
    pub phone: String,
}

/// 手机号登录响应（提示用户查收验证码）。
#[derive(Debug, Serialize)]
pub struct LoginPhoneResponse {
    /// 下一步操作提示
    pub tip: String,
}

/// POST /api/{platform}/login/verify 请求体：提交短信验证码。
#[derive(Debug, Deserialize)]
pub struct LoginVerifyRequest {
    /// 收到的验证码
    pub code: String,
}

/// 验证码校验结果。
#[derive(Debug, Serialize)]
pub struct LoginVerifyResponse {
    /// 校验后是否已登录
    pub logged_in: bool,
}

/// POST /api/doubao/debug/html 请求体：enabled 缺省时切换当前状态
#[derive(Debug, Deserialize)]
pub struct DebugHtmlRequest {
    pub enabled: Option<bool>,
}

/// 调试模式（HTML trace）当前状态
#[derive(Debug, Serialize)]
pub struct DebugHtmlResponse {
    pub debug_html: bool,
}

/// POST /api/{platform}/login/cookie 请求体：导入浏览器导出的 cookie。
/// `cookies` 接受两种形态：
/// - 字符串：EditThisCookie / Cookie-Editor 扩展导出的 JSON 数组文本，
///   或每行一条 `name=value; Domain=...; Path=...` 的 Set-Cookie 串
/// - JSON 数组：直接内联上述数组
#[derive(Debug, Deserialize)]
pub struct ImportCookiesRequest {
    pub cookies: serde_json::Value,
}

/// cookie 导入结果
#[derive(Debug, Serialize)]
pub struct ImportCookiesResponse {
    /// 成功写入 cookie jar 的条数
    pub imported: usize,
    /// 导入并刷新页面后的登录态校验结果
    pub logged_in: bool,
    pub message: String,
}

/// 通用错误响应体。
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// 人类可读的错误信息
    pub error: String,
}

/// 通用成功响应体。
#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

/// Worker 命令。单一任务持有 Page，对豆包的所有操作天然串行化，
/// 避免多个 HTTP 请求同时挤同一个账号会话导致互相串台。
pub enum Command {
    /// 发送问题并等待回复；`chunk_tx` 为 `Some` 时以增量分片流式下发
    Ask {
        question: String,
        /// 流式模式时下发增量分片；阻塞模式为 None
        chunk_tx: Option<mpsc::Sender<String>>,
        reply_tx: oneshot::Sender<anyhow::Result<AiReply>>,
    },
    /// 查询当前登录态与二维码状态
    Status {
        reply_tx: oneshot::Sender<StatusResponse>,
    },
    /// 打开登录弹窗并获取二维码
    LoginQr {
        reply_tx: oneshot::Sender<anyhow::Result<QrResponse>>,
    },
    /// 发起手机号登录（发送验证码）
    LoginPhone {
        phone: String,
        reply_tx: oneshot::Sender<anyhow::Result<LoginPhoneResponse>>,
    },
    /// 提交短信验证码完成登录
    LoginVerify {
        code: String,
        reply_tx: oneshot::Sender<anyhow::Result<LoginVerifyResponse>>,
    },
    /// 重置 worker（清空会话状态并重新加载页面）
    Reset {
        reply_tx: oneshot::Sender<anyhow::Result<()>>,
    },
    /// 返回当前页面调试信息
    Debug {
        reply_tx: oneshot::Sender<String>,
    },
    /// 导出指定 URL（缺省为当前页）的 HTML
    DumpHtml {
        url: Option<String>,
        reply_tx: oneshot::Sender<String>,
    },
    /// 开/关/切换调试模式（开启后每步操作渲染完都把 HTML 存到项目 html/ 目录）
    SetDebugHtml {
        enabled: Option<bool>,
        reply_tx: oneshot::Sender<DebugHtmlResponse>,
    },
    /// 截图，返回 PNG 文件路径
    Screenshot {
        reply_tx: oneshot::Sender<anyhow::Result<String>>,
    },
    /// 在页面坐标 (x, y) 处点击
    ClickAt {
        x: f64,
        y: f64,
        reply_tx: oneshot::Sender<anyhow::Result<()>>,
    },
    /// 求值任意 JS，返回 JSON 字符串
    EvalJs {
        js: String,
        reply_tx: oneshot::Sender<String>,
    },
    /// 导入浏览器导出的 cookie（JSON 数组文本或 Set-Cookie 行），
    /// 刷新页面校验登录态后持久化。用于无扫码/密码通道的平台
    /// （如 ChatGPT 的 Google 认证账号：真实浏览器登录后导出导入）。
    /// `local_storage`：CDP 授权顺带收集的页面 localStorage 条目——
    /// DeepSeek 等平台把会话凭证（userToken）存在 localStorage 而非
    /// cookie，仅靠 cookie 注入无法恢复登录态，需一并注入。
    ImportCookies {
        cookies: String,
        local_storage: Option<std::collections::HashMap<String, String>>,
        reply_tx: oneshot::Sender<anyhow::Result<ImportCookiesResponse>>,
    },
}
