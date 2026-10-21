//! HTTP 服务：AppState（平台 worker 通道表）+ 全部 REST 端点 + 服务装配。
//!
//! 供两种复用方式：
//! - [`configure_routes`]：把 `/healthz`、`/api/*`、`/v1/*` 全部路由嵌入任意
//!   actix-web App（App 需挂载 [`web::Data<AppState>`] 与
//!   [`crate::auth::RequireAuth`]）。
//! - [`serve`]：一键启动完整服务（含认证中间件），bin 入口即用此函数。

use std::collections::HashMap;

use actix_web::{web, App, HttpResponse, HttpServer};
use bytes::Bytes;
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::models::*;

/// 应用状态：各平台 worker 命令通道（platform name → sender）+ 可选 Bearer 认证。
pub struct AppState {
    /// 各平台 worker 命令通道（platform name → sender）
    pub workers: HashMap<String, mpsc::Sender<Command>>,
    /// 非空时启用 Bearer 认证（见 auth.rs；/healthz 豁免）
    pub token: Option<String>,
}

impl AppState {
    /// 取平台命令通道；未知平台返回 404 响应
    pub fn worker(
        &self,
        platform: &str,
    ) -> Result<mpsc::Sender<Command>, HttpResponse> {
        self.workers.get(platform).cloned().ok_or_else(|| {
            HttpResponse::NotFound().json(ErrorResponse {
                error: format!(
                    "未知平台: {platform}（已启用: {}）",
                    self.enabled_list()
                ),
            })
        })
    }

    /// 已启用平台名称列表（逗号分隔，按字母排序），用于错误提示。
    pub fn enabled_list(&self) -> String {
        let mut names: Vec<&str> = self.workers.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names.join(",")
    }
}

/// 注册全部路由（/healthz、/api/*、/v1/*）。
/// 嵌入方需自行挂载 `web::Data<AppState>`（并按需 wrap
/// [`crate::auth::RequireAuth`]）。
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/healthz", web::get().to(healthz))
        .route("/api/platforms", web::get().to(platforms))
        .route("/api/{platform}/ask", web::post().to(ask))
        .route("/api/{platform}/status", web::get().to(status))
        .route("/api/{platform}/login", web::post().to(login))
        .route("/api/{platform}/login/cookie", web::post().to(login_cookie))
        .route("/api/{platform}/login/phone", web::post().to(login_phone))
        .route("/api/{platform}/login/verify", web::post().to(login_verify))
        .route("/api/{platform}/reset", web::post().to(reset))
        .route("/api/{platform}/debug", web::get().to(debug))
        .route("/api/{platform}/debug/html", web::post().to(debug_html_toggle))
        .route("/api/{platform}/dump-html", web::get().to(dump_html))
        .route("/api/{platform}/screenshot", web::get().to(screenshot))
        .route("/api/{platform}/click", web::post().to(click_at))
        .route("/api/{platform}/eval", web::post().to(eval_js))
        // OpenAI / Anthropic 协议兼容端点
        .route("/v1/models", web::get().to(crate::compat::list_models))
        .route(
            "/v1/chat/completions",
            web::post().to(crate::compat::chat_completions),
        )
        .route("/v1/messages", web::post().to(crate::compat::messages));
}

/// 一键启动完整服务（RequireAuth 中间件 + AppState + 全部路由）。
/// bin 入口与需要整套网关 HTTP API 的嵌入方均可直接调用。
pub async fn serve(host: &str, port: u16, state: web::Data<AppState>) -> anyhow::Result<()> {
    HttpServer::new(move || {
        App::new()
            .wrap(crate::auth::RequireAuth)
            .app_data(state.clone())
            .configure(configure_routes)
    })
    .bind((host, port))
    .map_err(|e| anyhow::anyhow!("bind({host}:{port}) 失败: {e}"))?
    .run()
    .await
    .map_err(|e| anyhow::anyhow!("server run 失败: {e}"))
}

/// 健康检查：始终返回 `{"status":"ok"}`，且不受 Bearer 认证限制。
async fn healthz() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

/// 已启用平台列表 + 各自登录态。
/// 并行查询且每个平台 3s 超时：单个 worker 卡顿（如正在 ask）不拖垮整个列表。
async fn platforms(state: web::Data<AppState>) -> HttpResponse {
    let mut names: Vec<String> = state.workers.keys().cloned().collect();
    names.sort();
    let mut queries = Vec::new();
    for name in names {
        let Some(tx) = state.workers.get(&name) else { continue };
        let tx = tx.clone();
        queries.push(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            if tx.send(Command::Status { reply_tx }).await.is_ok() {
                if let Ok(Ok(r)) = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    reply_rx,
                )
                .await
                {
                    return serde_json::json!({
                        "name": name,
                        "logged_in": r.logged_in,
                        "qr_state": r.qr_state,
                    });
                }
            }
            serde_json::json!({
                "name": name,
                "logged_in": false,
                "qr_state": "unreachable",
            })
        });
    }
    let list = futures::future::join_all(queries).await;
    HttpResponse::Ok().json(serde_json::json!({ "platforms": list }))
}

/// POST /api/{platform}/ask：向平台发起一次提问。
///
/// `stream=false`（默认）时阻塞至回答完整；`stream=true` 时返回
/// `text/event-stream`，逐块下发增量，并在网关侧失败时追加
/// `[gateway-error]` 尾事件，便于客户端区分正常结束与异常。
async fn ask(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<AskRequest>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let stream = req.stream.unwrap_or(false);
    if stream {
        let (chunk_tx, chunk_rx) = mpsc::channel::<String>(64);
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = tx
            .send(Command::Ask {
                question: req.question.clone(),
                chunk_tx: Some(chunk_tx),
                reply_tx,
            })
            .await;
        // 合并通道：chunk 原样透传；结果失败时补一行 [gateway-error] 尾事件，
        // 客户端才能区分「答完」与「网关侧失败」（此前错误只进日志、流静默断掉）
        let (out_tx, out_rx) = mpsc::channel::<String>(64);
        actix_web::rt::spawn(async move {
            let mut chunk_rx = chunk_rx;
            while let Some(chunk) = chunk_rx.recv().await {
                if out_tx.send(chunk).await.is_err() {
                    return;
                }
            }
            let tail = match reply_rx.await {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => {
                    tracing::warn!("[{platform}] ask 失败: {e}");
                    Some(format!("[gateway-error] {e}"))
                }
                Err(_) => {
                    tracing::warn!("[{platform}] worker 已退出");
                    Some("[gateway-error] worker 已退出".to_string())
                }
            };
            if let Some(t) = tail {
                let _ = out_tx.send(t).await;
            }
        });
        let body = ReceiverStream::new(out_rx).map(|chunk| {
            // 结构化事件只输出文本部分（保持旧版纯文本 SSE 行为）；纯文本原样输出
            let text = match crate::common::chunk::decode_chunk(&chunk) {
                Some((_, t)) => t,
                None => chunk,
            };
            Ok::<_, actix_web::Error>(Bytes::from(format!("data: {text}\n\n")))
        });
        HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(("Cache-Control", "no-cache"))
            .insert_header(("X-Accel-Buffering", "no"))
            .streaming(body)
    } else {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = tx
            .send(Command::Ask {
                question: req.question.clone(),
                chunk_tx: None,
                reply_tx,
            })
            .await;
        match reply_rx.await {
            Ok(Ok(ai)) => HttpResponse::Ok().json(AskResponse {
                reply: ai.reply,
                thinking: ai.thinking,
                search_queries: ai.search_queries,
                references: ai.references,
            }),
            Ok(Err(e)) => {
                HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
            }
            Err(_) => HttpResponse::InternalServerError()
                .json(ErrorResponse { error: "worker 已退出".into() }),
        }
    }
}

#[derive(serde::Deserialize)]
struct EvalJsRequest {
    js: String,
}

/// POST /api/{platform}/eval：在页面上下文执行任意 JS，返回 JSON 字符串结果。
async fn eval_js(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<EvalJsRequest>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::EvalJs { js: req.js.clone(), reply_tx }).await;
    match reply_rx.await {
        Ok(result) => HttpResponse::Ok().json(serde_json::json!({"result": result})),
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// GET /api/{platform}/status：查询登录态与登录二维码状态。
async fn status(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::Status { reply_tx }).await;
    match reply_rx.await {
        Ok(r) => HttpResponse::Ok().json(r),
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// POST /api/{platform}/login：打开登录弹窗并生成二维码。
async fn login(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::LoginQr { reply_tx }).await;
    match reply_rx.await {
        Ok(Ok(qr)) => HttpResponse::Ok().json(qr),
        Ok(Err(e)) => {
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// POST /api/{platform}/login/cookie：导入真实浏览器导出的 cookie。
/// 当前仅 ChatGPT 实现（Google 认证账号的唯一可靠登录通道）；
/// 其余平台返回不支持错误。
async fn login_cookie(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<ImportCookiesRequest>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    // cookies 字段兼容字符串（扩展导出的 JSON 数组文本 / Set-Cookie 行）
    // 与直接内联 JSON 数组两种形态
    let cookies = match &req.cookies {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(_) => req.cookies.to_string(),
        _ => {
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "cookies 字段须为字符串（导出文本）或 cookie 对象数组".into(),
            })
        }
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx
        .send(Command::ImportCookies {
            cookies,
            local_storage: None,
            reply_tx,
        })
        .await;
    match reply_rx.await {
        Ok(Ok(r)) => HttpResponse::Ok().json(r),
        Ok(Err(e)) => {
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// POST /api/{platform}/login/phone：发起手机号登录（发送验证码）。
async fn login_phone(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<LoginPhoneRequest>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx
        .send(Command::LoginPhone {
            phone: req.phone.clone(),
            reply_tx,
        })
        .await;
    match reply_rx.await {
        Ok(Ok(r)) => HttpResponse::Ok().json(r),
        Ok(Err(e)) => {
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// POST /api/{platform}/login/verify：提交短信验证码完成登录。
async fn login_verify(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<LoginVerifyRequest>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx
        .send(Command::LoginVerify {
            code: req.code.clone(),
            reply_tx,
        })
        .await;
    match reply_rx.await {
        Ok(Ok(r)) => HttpResponse::Ok().json(r),
        Ok(Err(e)) => {
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// POST /api/{platform}/reset：重置 worker（清空会话状态并重新加载页面）。
async fn reset(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::Reset { reply_tx }).await;
    match reply_rx.await {
        Ok(Ok(())) => HttpResponse::Ok().json(OkResponse { ok: true }),
        Ok(Err(e)) => {
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// GET /api/{platform}/debug：返回 worker 当前的调试信息（页面 URL、登录态等）。
///
/// worker 侧返回的是手工拼接的 JSON 字符串，这里先解析校验；
/// 非法 JSON 会被包装为 `{"raw": ...}`，保证响应始终合法。
async fn debug(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::Debug { reply_tx }).await;
    match reply_rx.await {
        // worker 侧是手工拼接的 JSON 串，异常路径（错误消息含引号等）可能拼出
        // 非法 JSON；先解析校验，失败则包成 {"raw": ...} 保证响应永远合法
        Ok(info) => match serde_json::from_str::<serde_json::Value>(&info) {
            Ok(v) => HttpResponse::Ok().json(v),
            Err(_) => HttpResponse::Ok().json(serde_json::json!({ "raw": info })),
        },
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// 开/关/切换调试模式。
/// - `POST /api/{platform}/debug/html`            → 切换当前状态
/// - `POST /api/{platform}/debug/html {"enabled": true}`  → 显式开启 / 关闭
/// 开启后每步操作（导航/点击/填入/发送等）页面渲染完，自动把 HTML 存到项目 `html/` 目录。
async fn debug_html_toggle(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: Option<web::Json<DebugHtmlRequest>>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let enabled = req.and_then(|r| r.enabled);
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx
        .send(Command::SetDebugHtml { enabled, reply_tx })
        .await;
    match reply_rx.await {
        Ok(r) => HttpResponse::Ok().json(r),
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// GET /api/{platform}/dump-html：导出页面 HTML 到临时文件，返回文件路径。
///
/// 可选查询参数 `url` 指定页面；缺省导出当前页。
async fn dump_html(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let url = query.get("url").cloned();
    let _ = tx.send(Command::DumpHtml { url, reply_tx }).await;
    match reply_rx.await {
        Ok(path) => HttpResponse::Ok().json(serde_json::json!({"html_file": path})),
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

/// GET /api/{platform}/screenshot：截图并返回 PNG 文件路径。
async fn screenshot(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::Screenshot { reply_tx }).await;
    match reply_rx.await {
        Ok(Ok(path)) => HttpResponse::Ok().json(serde_json::json!({"screenshot": path})),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}

#[derive(serde::Deserialize)]
struct ClickAtRequest {
    x: f64,
    y: f64,
}

/// POST /api/{platform}/click：在页面坐标 `(x, y)` 处点击。
///
/// 坐标必须是非负有限数字，避免把 NaN/Infinity 传给浏览器引擎。
async fn click_at(
    state: web::Data<AppState>,
    path: web::Path<String>,
    req: web::Json<ClickAtRequest>,
) -> HttpResponse {
    let platform = path.into_inner();
    let tx = match state.worker(&platform) {
        Ok(tx) => tx,
        Err(resp) => return resp,
    };
    if !req.x.is_finite() || !req.y.is_finite() || req.x < 0.0 || req.y < 0.0 {
        return HttpResponse::BadRequest().json(ErrorResponse {
            error: "坐标必须是非负有限数字".into(),
        });
    }
    let (reply_tx, reply_rx) = oneshot::channel();
    let _ = tx.send(Command::ClickAt { x: req.x, y: req.y, reply_tx }).await;
    match reply_rx.await {
        Ok(Ok(())) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
        Err(_) => HttpResponse::InternalServerError()
            .json(ErrorResponse { error: "worker 已退出".into() }),
    }
}
