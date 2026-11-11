//! 中央 hub 推送连接管理器。
//!
//! 桌面端/Web 端作为 channel 客户端与中央 hub 服务器（kit/channel-server
//! 独立部署）保持一条 `channel.v1/ChannelService/Open` 双向长连接：
//! - **在线统计**：首帧 `Hello`（携带匿名安装 ID + 设备信息快照）注册会话，
//!   连接存续即在线，服务端按安装 ID 去重；客户端每 25 秒发送一帧 `Heartbeat` 保活；
//! - **活动推送**：hub 通过流的 `Push` 下行推送业务消息（payload 为 JSON），
//!   本服务解析为 [`PushEvent`] 经 `broadcast` 通道分发；
//!   桌面端由 src-tauri 订阅该通道并 emit 给前端（`hub-push` 事件），
//!   Web 模式无 Tauri 事件层，前端保留 5 分钟轮询兜底。
//!
//! 重连策略：断流/连接失败按指数退避（5s 起、封顶 5 分钟、加抖动），
//! 流成功建立后重置退避；`HelloAck.success=false`（握手被拒）按契约断流走退避。
//! 任务类下行（`TaskAssign`/`TaskCancel`/`HelloAck.pending_tasks` 补发清单）
//! 暂不执行：`TaskAssign` 一律回拒绝回执 `TaskAck`，避免 hub 投递器按超时
//! 反复重投直至次数耗尽（升级类任务已并入 `TaskAssign(task_type=1)`，
//! 独立 `Upgrade` 消息已从协议废弃）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use channel_client::channel::{
    client_message::Payload as ClientPayload, server_message::Payload as ServerPayload,
    ClientMessage, DeviceInfo, Hello, Heartbeat, PushMessage, ServerMessage, TaskAck, TaskAssign,
};
use channel_client::{ChannelClient, ChannelClientConfig, ChannelError};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use vortex_store::db;

/// 默认 hub gRPC 地址：公网域名 + TLS（证书由服务端反代终结，
/// 客户端信任系统根证书零配置），与 geo-pet 连 `https://geo.htui.cc` 同款；
/// 本地明文冒烟显式写 `http://host:port`。
pub const DEFAULT_PUSH_ENDPOINT: &str = "https://hub.htui.cc";

/// 退避下限：首次重连等待。
const BACKOFF_MIN: Duration = Duration::from_secs(5);
/// 退避上限。
const BACKOFF_MAX: Duration = Duration::from_secs(300);
/// 心跳间隔（服务端超过 60 秒无上行消息会判定会话超时）。
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
/// Hello 的应用标识（服务端区分接入方）。
const APP_KEY: &str = "vortex";
/// 无业务语义的默认消息类型（不映射为前端 kind）。
const DEFAULT_MESSAGE_TYPE: &str = "notify";

/// 推送给前端的事件（下行 `Push` 消息的投影）。
///
/// 序列化为 camelCase JSON，经 Tauri `emit("hub-push")` 下发，
/// 前端 `useNotifications.ingestPush` 消费。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushEvent {
    /// 消息 id（服务器生成），前端按其去重（重连重放不重复弹）。
    pub id: String,
    /// 标题（短句）。
    pub title: String,
    /// 正文。
    pub body: String,
    /// 展示级别："info" / "warning" / "error"。
    pub level: String,
    /// 业务类型（如 "free-token" / "quota" / "update"），可空。
    pub kind: String,
    /// 点击跳转链接，可空；前端无链接时默认跳薅羊毛页。
    pub url: String,
    /// 服务器生成时刻（Unix 毫秒）。
    pub timestamp_ms: i64,
}

/// 推送连接配置（启动时解析一次，改动需重启生效——对齐 tls_enabled 先例）。
#[derive(Debug, Clone)]
pub struct PushConfig {
    /// 是否启用推送连接。
    pub enabled: bool,
    /// hub gRPC 地址（host:port / http(s)://host:port）。
    pub endpoint: String,
    /// Bearer 令牌（复用 hub_token 配置）。
    pub token: Option<String>,
    /// 匿名安装 ID（`Hello.device_id`，服务端按其去重在线统计）。
    pub install_id: String,
}

/// 推送事件广播发送端：AppState 持有，src-tauri 订阅后 emit 给前端。
pub type PushEventTx = broadcast::Sender<PushEvent>;

/// 最近推送事件内存缓冲：AppState 持有，供前端启动/刷新时回填收件箱
/// （推送不经过轮询通道，webview 未挂载期间的推送靠它兜底恢复）。
pub type PushHistory = Arc<std::sync::Mutex<std::collections::VecDeque<PushEvent>>>;

/// 连接状态标志：`HelloAck.success=true` 时置 true，断流/连接失败时置 false。
/// AppState 持有，`GET /api/push-events` 随事件一并返回，供前端/运维确认
/// 通道连通性（对齐 geo-pet 的 hello-ack 状态上报）。
pub type PushConnected = Arc<AtomicBool>;

/// 历史缓冲容量。
const HISTORY_CAP: usize = 50;

/// 创建推送事件广播通道（容量 16：前端消费慢时丢最旧事件，通知类消息可容忍）。
pub fn push_channel() -> PushEventTx {
    broadcast::channel(16).0
}

/// 创建推送历史缓冲。
pub fn push_history() -> PushHistory {
    Arc::new(std::sync::Mutex::new(std::collections::VecDeque::with_capacity(HISTORY_CAP)))
}

/// 创建连接状态标志（初始 false，未连接）。
pub fn push_connected() -> PushConnected {
    Arc::new(AtomicBool::new(false))
}

/// 解析推送配置：env 显式覆盖 > 数据库持久化设置 > 默认值。
///
/// - 开关：`VORTEX_PUSH_ENABLED`（"false"/"0" 关）> `settings.general.pushEnabled` > true
/// - 地址：`VORTEX_PUSH_ENDPOINT` > `settings.general.pushEndpoint` > [`DEFAULT_PUSH_ENDPOINT`]
/// - 令牌：复用 `hub_token`（`VORTEX_HUB_TOKEN`）
///
/// 匿名安装 ID 持久化在 `key_value("settings","push")`，首次生成 uuid。
/// # 参数
/// - `cfg`：应用配置（取 hub_token）
/// - `pool`：数据库连接池（读设置 + 持久化安装 ID）
pub fn resolve_push_config(
    cfg: &vortex_store::config::AppConfig,
    pool: &db::core::DbPool,
) -> PushConfig {
    // env 显式设置优先
    let env_enabled = std::env::var("VORTEX_PUSH_ENABLED").ok().map(|v| v != "false" && v != "0");
    let env_endpoint = std::env::var("VORTEX_PUSH_ENDPOINT").ok().filter(|s| !s.trim().is_empty());

    let (mut enabled, endpoint, install_id) = {
        let Ok(conn) = db::core::get_conn(pool) else {
            // 数据库不可用：仅 env / 默认值，无持久化安装 ID（本进程内随机，重启即变）
            return PushConfig {
                enabled: env_enabled.unwrap_or(true),
                endpoint: env_endpoint.unwrap_or_else(|| DEFAULT_PUSH_ENDPOINT.to_string()),
                token: non_empty(cfg.hub_token.clone()),
                install_id: ephemeral_id(),
            };
        };
        let general = db::settings::get(&conn, "settings", "general")
            .ok()
            .flatten()
            .unwrap_or(serde_json::json!({}));
        let db_enabled = general.get("pushEnabled").and_then(|v| v.as_bool());
        let db_endpoint = general
            .get("pushEndpoint")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let install_id = {
            const NS: &str = "settings";
            const KEY: &str = "push";
            let existing = db::settings::get(&conn, NS, KEY)
                .ok()
                .flatten()
                .and_then(|v| v.get("installId").and_then(|s| s.as_str()).map(String::from))
                .filter(|s| !s.is_empty());
            match existing {
                Some(id) => id,
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    // 持久化失败不阻断连接：本进程内沿用，下次启动重新生成
                    if let Err(e) = db::settings::set(&conn, NS, KEY, &serde_json::json!({ "installId": id })) {
                        log::warn!("[push] 安装 ID 持久化失败（本次进程内临时使用）: {e}");
                    }
                    id
                }
            }
        };
        (db_enabled, db_endpoint, install_id)
    };
    if let Some(env) = env_enabled {
        enabled = Some(env);
    }
    let endpoint = env_endpoint.or(endpoint).unwrap_or_else(|| DEFAULT_PUSH_ENDPOINT.to_string());

    PushConfig {
        enabled: enabled.unwrap_or(true),
        endpoint,
        token: non_empty(cfg.hub_token.clone()),
        install_id,
    }
}

/// 空串归一为 None（hub_token 为普通 String，空 = 未设置）。
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// 数据库不可用时的临时安装 ID（进程内唯一即可）。
fn ephemeral_id() -> String {
    format!("ephemeral-{}", uuid::Uuid::new_v4())
}

/// 规范 endpoint 为 channel 客户端可用的 URL：剥掉路径与 query，
/// 无 scheme 时补 `https://`（部署走公网域名 + TLS，对齐 geo-pet 客户端；
/// 明文本地测试需显式写 `http://host:port`）。
fn normalize_endpoint(endpoint: &str) -> String {
    let ep = endpoint.trim().trim_end_matches('/');
    let base = if let Some((scheme, rest)) = ep.split_once("://") {
        format!("{scheme}://{rest}")
    } else {
        format!("https://{ep}")
    };
    let (scheme, rest) = base.split_once("://").expect("已保证含 scheme");
    let host = rest.split(['/', '?']).next().unwrap_or(rest);
    format!("{scheme}://{host}")
}

/// 启动推送连接循环（长驻，失败自动重连）。
///
/// 在 `start_api_server` 的 actix 运行时 spawn；`enabled=false` 时直接返回。
/// # 参数
/// - `cfg`：推送连接配置
/// - `tx`：推送事件广播发送端（AppState.push_events）
/// - `history`：最近推送内存缓冲（AppState.push_history）
/// - `connected`：连接状态标志（AppState.push_connected）
pub async fn run_push_loop(
    cfg: PushConfig,
    tx: PushEventTx,
    history: PushHistory,
    connected: PushConnected,
) {
    if !cfg.enabled {
        log::info!("[push] 活动推送未启用（settings.general.pushEnabled 或 VORTEX_PUSH_ENABLED 关闭）");
        return;
    }
    let endpoint = normalize_endpoint(&cfg.endpoint);
    let token = cfg.token.unwrap_or_default();
    log::info!(
        "[push] 启动 hub 推送连接: {endpoint}（安装 ID: {}，token={}）",
        cfg.install_id,
        if token.is_empty() { "无" } else { "有" }
    );

    let mut backoff = BACKOFF_MIN;
    loop {
        // 每轮新建出站通道：旧 ReceiverStream 已随上一条流消费，不可复用
        let (out_tx, out_rx) = mpsc::channel::<ClientMessage>(32);
        match connect_and_stream(&endpoint, &token, &cfg.install_id, out_tx, out_rx, &tx, &history, &connected, &mut backoff).await {
            Ok(()) => {
                // 服务端主动断流（如重启/维护）：视为一次断线，走退避重连
                // （backoff 已在流建立成功时重置，稳定长连接断开后立即从最小退避开始）
                log::info!("[push] hub 推送流结束，准备重连");
            }
            Err(e) => {
                log::warn!("[push] hub 连接异常: {e}（{} 后重试）", format_duration(backoff));
            }
        }
        connected.store(false, Ordering::Relaxed);
        // 抖动 ±20%：避免服务端重启后所有客户端同一瞬间重连
        let wait = backoff.mul_f64(0.8 + jitter_ratio());
        tokio::time::sleep(wait).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// 建立连接、发送 Hello 握手并消费双向流直至断开。
///
/// 返回 `Ok(())` 表示流正常结束（服务端关闭）；`Err` 携带连接/流错误。
/// 流建立成功时把 `backoff` 重置为最小值（长连接断开应立即快速重试）。
#[allow(clippy::too_many_arguments)]
async fn connect_and_stream(
    endpoint: &str,
    token: &str,
    install_id: &str,
    out_tx: mpsc::Sender<ClientMessage>,
    out_rx: mpsc::Receiver<ClientMessage>,
    tx: &PushEventTx,
    history: &PushHistory,
    connected: &PushConnected,
    backoff: &mut Duration,
) -> Result<(), String> {
    let mut client = match ChannelClient::connect(ChannelClientConfig::new(endpoint, token)).await {
        Ok(c) => c,
        Err(e) => {
            if is_config_error(&e) {
                log::error!(
                    "[push] hub 连接配置错误（请检查 VORTEX_PUSH_ENDPOINT / VORTEX_HUB_TOKEN）: {}",
                    err_chain(&e)
                );
            }
            return Err(err_chain(&e));
        }
    };

    // 首帧 Hello（安装 ID + 完整设备快照，服务端登记与在线去重用）。
    // 设备信息采集含系统命令调用（阻塞），移到阻塞线程池执行（对齐 geo-pet）
    let version = env!("CARGO_PKG_VERSION").to_string();
    let snap = tokio::task::spawn_blocking(super::device_info::snapshot)
        .await
        .unwrap_or_default();
    let hello = ClientMessage {
        payload: Some(ClientPayload::Hello(Hello {
            device_id: install_id.to_string(),
            app_key: APP_KEY.to_string(),
            version: version.clone(),
            // 协议字段 4（客户端自报 ip）已废弃：服务端恒取传输层对端地址；
            // 登录用户资料（字段 6）vortex 暂无账号体系，不上报。
            device_info: Some(DeviceInfo {
                device_brand: snap.device_brand,
                device_model: snap.device_model,
                device_type: snap.device_type,
                screen_width: snap.screen_width,
                screen_height: snap.screen_height,
                os_name: snap.os_name,
                os_version: snap.os_version,
                wifi_enabled: snap.wifi_enabled,
                bluetooth_enabled: snap.bluetooth_enabled,
                host_version: version,
            }),
            user_info: None,
        })),
    };
    out_tx.send(hello).await.map_err(|e| format!("发送 Hello 失败: {e}"))?;

    // 打开双向流（消费 out_rx；下行等待 HelloAck / 心跳应答 / 推送）
    let mut inbound = match client.open(ReceiverStream::new(out_rx)).await {
        Ok(s) => s,
        Err(e) => {
            if is_config_error(&e) {
                log::error!("[push] hub 打开流被拒（请检查服务端配置）: {}", err_chain(&e));
            }
            return Err(err_chain(&e));
        }
    };
    *backoff = BACKOFF_MIN;
    log::info!("[push] 已连接 hub，等待握手应答");

    let mut heartbeat_seq = 0i64;
    let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
    ticker.tick().await; // 跳过首次立即触发

    // 收发循环
    loop {
        tokio::select! {
            msg = inbound.message() => match msg {
                Ok(Some(msg)) => {
                    if !handle_message(&out_tx, tx, history, connected, msg).await {
                        // 握手被拒（HelloAck.success=false）：按契约关闭流，退避重连
                        break Err("握手被拒（HelloAck.success=false）".to_string());
                    }
                }
                Ok(None) => break Ok(()),
                Err(e) => break Err(format!("gRPC 流错误: {}", err_chain(&e))),
            },
            _ = ticker.tick() => {
                heartbeat_seq += 1;
                let hb = ClientMessage {
                    payload: Some(ClientPayload::Heartbeat(Heartbeat { seq: heartbeat_seq })),
                };
                if out_tx.send(hb).await.is_err() {
                    // 流已断开（接收端随断流关闭），走重连
                    break Ok(());
                }
            }
        }
    }
}

/// 连接级配置错误（重连不会自愈，需用户检查配置，用 error 级别记录）。
fn is_config_error(err: &ChannelError) -> bool {
    matches!(
        err,
        ChannelError::InvalidEndpoint(_) | ChannelError::InvalidAuthMetadata(_) | ChannelError::TlsConfig(_)
    )
}

/// 展开错误源链：transport error 这类包装错误需溯源到根因（TLS/DNS/HTTP2 等）。
fn err_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut chain = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        chain.push_str(" ← ");
        chain.push_str(&s.to_string());
        src = s.source();
    }
    chain
}

/// 抖动比例（0~0.2，叠加在 0.8 倍基础退避上，整体 ±20%）。
fn jitter_ratio() -> f64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 % 400)
        .unwrap_or(0)) as f64
        / 1000.0
}

/// 处理单条下行消息：仅 `Push` 转换分发（广播 + 历史缓冲），握手应答决定会话去留。
/// 返回 false 表示会话应终止（握手被拒），由调用方断流重连。
/// `TaskAssign`（含 `HelloAck.pending_tasks` 补发清单）暂不执行，一律回拒绝回执。
async fn handle_message(
    out_tx: &mpsc::Sender<ClientMessage>,
    tx: &PushEventTx,
    history: &PushHistory,
    connected: &PushConnected,
    msg: ServerMessage,
) -> bool {
    match msg.payload {
        Some(ServerPayload::HelloAck(ack)) => {
            if ack.success {
                // 服务端确认会话：对外暴露「已连接」（对齐 geo-pet hello-ack 状态上报）
                connected.store(true, Ordering::Relaxed);
                log::info!("[push] hub 握手成功（在线统计生效）");
                // 重连补发清单：任务已随握手应答标记已投递，同样拒绝避免超时重投
                if !ack.pending_tasks.is_empty() {
                    log::info!(
                        "[push] 重连补发任务 {} 个（暂不执行，回拒绝回执）",
                        ack.pending_tasks.len()
                    );
                    for task in &ack.pending_tasks {
                        reject_task(out_tx, task).await;
                    }
                }
            } else {
                log::error!("[push] hub 握手被拒: {}", ack.message);
            }
            ack.success
        }
        Some(ServerPayload::HeartbeatAck(_)) => true,
        Some(ServerPayload::Push(push)) => {
            let event = parse_push_event(&push);
            log::info!("[push] 收到活动推送: {}（{}）", event.title, event.kind);
            // 入内存历史（前端启动/刷新时经 GET /api/push-events 回填收件箱）
            if let Ok(mut q) = history.lock() {
                q.push_back(event.clone());
                while q.len() > HISTORY_CAP {
                    q.pop_front();
                }
            }
            // 无订阅者（Web 模式未接前端层）时 send 返回 Err，忽略即可
            let _ = tx.send(event);
            true
        }
        Some(ServerPayload::TaskAssign(task)) => {
            log::debug!(
                "[push] 收到任务下发（暂不执行，回拒绝回执）: id={} type={} attempt={}",
                task.task_id,
                task_type_name(task.task_type),
                task.attempt
            );
            reject_task(out_tx, &task).await;
            true
        }
        Some(ServerPayload::TaskCancel(cancel)) => {
            log::debug!("[push] 收到任务取消（暂不消费）: id={}", cancel.task_id);
            true
        }
        None => true,
    }
}

/// 回拒绝任务回执（vortex 暂不支持任务执行）。
///
/// 幂等键 `task_id + attempt` 需与服务端投递记录一致才生效，
/// 故原样回填 [`TaskAssign::attempt`]；回执后 hub 侧 `on_task_ack`
/// 收到 `success=false` 终止该任务投递，不再按超时重投。
async fn reject_task(out_tx: &mpsc::Sender<ClientMessage>, task: &TaskAssign) {
    let ack = ClientMessage {
        payload: Some(ClientPayload::TaskAck(TaskAck {
            task_id: task.task_id,
            attempt: task.attempt,
            success: false,
            message: "vortex 暂不支持任务执行".to_string(),
        })),
    };
    if out_tx.send(ack).await.is_err() {
        log::debug!("[push] 任务 {} 拒绝回执发送失败（流已关闭）", task.task_id);
    }
}

/// `TaskType` 枚举值 → 名称（日志用；协议契约：1 升级 / 2 配置 / 3 指令 / 4 检查更新）。
fn task_type_name(task_type: i32) -> &'static str {
    match task_type {
        1 => "upgrade",
        2 => "config",
        3 => "command",
        4 => "check-update",
        _ => "unspecified",
    }
}

/// 下行 `Push` 消息 → 前端 [`PushEvent`]。
///
/// payload 契约为 JSON 对象 `{"title":..,"body":..,"level":..,"url":..}`；
/// 兼容缺失字段：title 缺省为空、body 缺省取 payload 原文、level 缺省 info；
/// `message_type` 为业务类型（kind），`notify` 视为无类型；
/// `server_time` 为秒级时间戳，换算为毫秒（缺省回退当前时间）。
fn parse_push_event(push: &PushMessage) -> PushEvent {
    let message_type = push.message_type.trim();
    let kind = if message_type.is_empty() || message_type == DEFAULT_MESSAGE_TYPE {
        String::new()
    } else {
        message_type.to_string()
    };
    let (title, body, level, url) = parse_payload(&push.payload);
    PushEvent {
        id: push.message_id.clone(),
        title,
        body,
        level,
        kind,
        url,
        timestamp_ms: if push.server_time > 0 {
            push.server_time * 1000
        } else {
            now_ms()
        },
    }
}

/// 解析 payload 为 `(title, body, level, url)`，尽力兼容多种服务端格式：
/// - JSON 对象：title/msgTitle + body/content/message/msg/text + level + url/link；
/// - 包装对象：`{"data": {...}}` / `{"payload": {...}}` 等取内层继续解析；
/// - 多层编码：JSON 字符串里还装着一份 JSON（逐层剥开）；
/// - 纯文本：整串作为 body，title 留空。
fn parse_payload(payload: &str) -> (String, String, String, String) {
    /// 从 JSON 对象里按候选键取字符串/数字字段
    fn field(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
        let map = v.as_object()?;
        keys.iter().find_map(|k| {
            map.get(*k).and_then(|x| match x {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
        })
    }

    /// 归一化展示级别：warning/warn → warning，error/err → error，其余 info
    fn normalize_level(raw: Option<String>) -> String {
        match raw.map(|s| s.trim().to_lowercase()) {
            Some(s) if s == "warning" || s == "warn" => "warning".to_string(),
            Some(s) if s == "error" || s == "err" => "error".to_string(),
            _ => "info".to_string(),
        }
    }

    /// 从 JSON 值提取 (title, body, level, url)；字段独立提取，
    /// 均缺失时 body 回退原文
    fn extract(v: &serde_json::Value, raw: &str, depth: usize) -> (String, String, String, String) {
        match v {
            serde_json::Value::Object(_) => {
                let title = field(v, &["title", "msgTitle", "标题"]);
                let body = field(v, &["body", "content", "message", "msg", "text", "内容"]);
                let level = normalize_level(field(v, &["level", "severity"]));
                let url = field(v, &["url", "link", "jumpUrl"]).unwrap_or_default();
                if title.is_some() || body.is_some() {
                    return (
                        title.unwrap_or_default(),
                        body.unwrap_or_else(|| raw.to_string()),
                        level,
                        url,
                    );
                }
                // 包装层：data/payload/notice/result 取内层继续
                if depth < 3 {
                    for k in ["data", "payload", "notice", "result"] {
                        if let Some(inner) = v.get(k) {
                            let (t, b, l, u) = extract(inner, raw, depth + 1);
                            if !t.is_empty() || b != *raw {
                                return (t, b, l, u);
                            }
                        }
                    }
                }
                (String::new(), raw.to_string(), level, url)
            }
            // JSON 字符串：里面可能还装着一份 JSON（双层编码），继续剥
            serde_json::Value::String(s) if depth < 3 => {
                match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(inner) => extract(&inner, raw, depth + 1),
                    Err(_) => (String::new(), s.clone(), "info".to_string(), String::new()),
                }
            }
            _ => (String::new(), raw.to_string(), "info".to_string(), String::new()),
        }
    }

    let raw = payload.trim().to_string();
    if raw.is_empty() {
        return (String::new(), String::new(), "info".to_string(), String::new());
    }
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) => extract(&v, &raw, 0),
        Err(_) => (String::new(), raw, "info".to_string(), String::new()),
    }
}

/// 导出历史缓冲快照（新→旧序，前端收件箱直接消费）。
pub fn recent_events(history: &PushHistory) -> Vec<PushEvent> {
    match history.lock() {
        Ok(q) => q.iter().rev().cloned().collect(),
        Err(_) => Vec::new(),
    }
}

/// Unix 毫秒时间戳。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 退避时长的人类可读形式（日志用）。
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use channel_client::channel::HelloAck;

    fn push_message(message_type: &str, payload: &str) -> PushMessage {
        PushMessage {
            message_id: "m-1".into(),
            message_type: message_type.into(),
            payload: payload.into(),
            server_time: 1_720_000_000,
        }
    }

    #[test]
    fn push_payload_maps_all_fields() {
        let ev = parse_push_event(&push_message(
            "free-token",
            r#"{"title":"新活动","body":"OpenRouter 免费额度","level":"warning","url":"https://example.com"}"#,
        ));
        assert_eq!(ev.id, "m-1");
        assert_eq!(ev.title, "新活动");
        assert_eq!(ev.body, "OpenRouter 免费额度");
        assert_eq!(ev.level, "warning");
        assert_eq!(ev.kind, "free-token");
        assert_eq!(ev.url, "https://example.com");
        // server_time 为秒级时间戳，换算毫秒
        assert_eq!(ev.timestamp_ms, 1_720_000_000_000);
        // serde 序列化为 camelCase（前端契约）
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["timestampMs"], 1_720_000_000_000i64);
    }

    #[test]
    fn notify_type_and_missing_fields_fall_back() {
        let ev = parse_push_event(&push_message("notify", r#"{"title":"t"}"#));
        // notify 为无业务语义的默认类型，不映射为 kind
        assert_eq!(ev.kind, "");
        assert_eq!(ev.level, "info");
        assert_eq!(ev.url, "");
        assert_eq!(ev.title, "t");
    }

    #[test]
    fn plain_text_payload_becomes_body() {
        let ev = parse_push_event(&push_message("notice", "服务器维护通知"));
        assert_eq!(ev.title, "");
        assert_eq!(ev.body, "服务器维护通知");
        assert_eq!(ev.kind, "notice");
    }

    #[test]
    fn wrapped_and_double_encoded_payload_unwrapped() {
        let wrapped = parse_push_event(&push_message(
            "notify",
            r#"{"code":0,"data":{"title":"t3","body":"b3"}}"#,
        ));
        assert_eq!(wrapped.title, "t3");
        assert_eq!(wrapped.body, "b3");

        let encoded = parse_push_event(&push_message(
            "notify",
            r#""{\"title\":\"t2\",\"body\":\"b2\"}""#,
        ));
        assert_eq!(encoded.title, "t2");
        assert_eq!(encoded.body, "b2");
    }

    #[test]
    fn error_level_and_missing_server_time() {
        let mut m = push_message("notify", r#"{"level":"error","body":"紧急"}"#);
        m.server_time = 0;
        let ev = parse_push_event(&m);
        assert_eq!(ev.level, "error");
        assert!(ev.timestamp_ms > 0, "缺省 server_time 应回退当前时间");
    }

    #[tokio::test]
    async fn handle_push_broadcasts_and_buffers() {
        let (tx, mut rx) = broadcast::channel::<PushEvent>(4);
        let history = push_history();
        let connected = push_connected();
        let (out_tx, _out_rx) = mpsc::channel::<ClientMessage>(4);
        let msg = ServerMessage {
            payload: Some(ServerPayload::Push(push_message(
                "free-token",
                r#"{"title":"新活动","body":"正文"}"#,
            ))),
        };
        assert!(handle_message(&out_tx, &tx, &history, &connected, msg).await);
        // 业务推送不改变连接状态（连接状态只由 HelloAck 驱动）
        assert!(!connected.load(Ordering::Relaxed));
        let ev = rx.try_recv().unwrap();
        assert_eq!(ev.title, "新活动");
        assert_eq!(ev.kind, "free-token");
        // 同时入历史缓冲，recent_events 以新→旧导出
        let recent = recent_events(&history);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, "m-1");
    }

    #[tokio::test]
    async fn task_assign_rejected_with_ack() {
        let (tx, _rx) = broadcast::channel::<PushEvent>(4);
        let history = push_history();
        let connected = push_connected();
        let (out_tx, mut out_rx) = mpsc::channel::<ClientMessage>(4);
        let msg = ServerMessage {
            payload: Some(ServerPayload::TaskAssign(TaskAssign {
                task_id: 9,
                task_type: 1,
                attempt: 2,
                payload: "{}".into(),
            })),
        };
        assert!(handle_message(&out_tx, &tx, &history, &connected, msg).await);
        // 未执行的任务回拒绝回执：幂等键 task_id + attempt 原样回填
        let ack = out_rx.recv().await.unwrap();
        match ack.payload {
            Some(ClientPayload::TaskAck(ack)) => {
                assert_eq!(ack.task_id, 9);
                assert_eq!(ack.attempt, 2);
                assert!(!ack.success);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn task_type_names() {
        assert_eq!(task_type_name(1), "upgrade");
        assert_eq!(task_type_name(2), "config");
        assert_eq!(task_type_name(3), "command");
        assert_eq!(task_type_name(4), "check-update");
        assert_eq!(task_type_name(0), "unspecified");
        assert_eq!(task_type_name(99), "unspecified");
    }

    #[tokio::test]
    async fn hello_ack_rejection_terminates_session() {
        let (tx, _rx) = broadcast::channel::<PushEvent>(4);
        let history = push_history();
        let connected = push_connected();
        let (out_tx, _out_rx) = mpsc::channel::<ClientMessage>(4);
        let ack = HelloAck {
            success: false,
            message: "denied".into(),
            server_time: 0,
            pending_tasks: vec![],
        };
        let msg = ServerMessage {
            payload: Some(ServerPayload::HelloAck(ack)),
        };
        // 握手被拒：会话终止，连接状态保持 false
        assert!(!handle_message(&out_tx, &tx, &history, &connected, msg).await);
        assert!(!connected.load(Ordering::Relaxed));

        let ok = ServerMessage {
            payload: Some(ServerPayload::HelloAck(HelloAck {
                success: true,
                message: String::new(),
                server_time: 0,
                pending_tasks: vec![TaskAssign {
                    task_id: 5,
                    task_type: 3,
                    attempt: 1,
                    payload: String::new(),
                }],
            })),
        };
        // 握手成功：会话继续，连接状态置 true
        assert!(handle_message(&out_tx, &tx, &history, &connected, ok).await);
        assert!(connected.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn hello_ack_pending_tasks_rejected() {
        let (tx, _rx) = broadcast::channel::<PushEvent>(4);
        let history = push_history();
        let connected = push_connected();
        let (out_tx, mut out_rx) = mpsc::channel::<ClientMessage>(4);
        let msg = ServerMessage {
            payload: Some(ServerPayload::HelloAck(HelloAck {
                success: true,
                message: String::new(),
                server_time: 0,
                pending_tasks: vec![
                    TaskAssign {
                        task_id: 11,
                        task_type: 2,
                        attempt: 1,
                        payload: String::new(),
                    },
                    TaskAssign {
                        task_id: 12,
                        task_type: 4,
                        attempt: 3,
                        payload: String::new(),
                    },
                ],
            })),
        };
        assert!(handle_message(&out_tx, &tx, &history, &connected, msg).await);
        // 补发清单逐条拒绝，attempt 与服务端投递记录一致
        for (idx, (expected_id, expected_attempt)) in
            [(11i64, 1i32), (12, 3)].into_iter().enumerate()
        {
            let ack = out_rx.recv().await.unwrap();
            match ack.payload {
                Some(ClientPayload::TaskAck(ack)) => {
                    assert_eq!(ack.task_id, expected_id, "第 {} 条补发任务", idx + 1);
                    assert_eq!(ack.attempt, expected_attempt);
                    assert!(!ack.success);
                }
                other => panic!("unexpected payload: {other:?}"),
            }
        }
        assert!(out_rx.try_recv().is_err(), "不应有多余回执");
    }

    #[test]
    fn endpoint_normalization() {
        assert_eq!(normalize_endpoint("hub.htui.cc:9090"), "https://hub.htui.cc:9090");
        assert_eq!(normalize_endpoint("https://hub.htui.cc/"), "https://hub.htui.cc");
        assert_eq!(normalize_endpoint("https://hub.htui.cc/base"), "https://hub.htui.cc");
        assert_eq!(normalize_endpoint("http://127.0.0.1:19999/"), "http://127.0.0.1:19999");
        assert_eq!(normalize_endpoint(" localhost:19999 "), "https://localhost:19999");
    }
}
