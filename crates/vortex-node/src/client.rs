//! 远程节点 tonic 客户端封装（极简：在线状态 + 消息推送）。
//!
//! 用法：
//! ```no_run
//! use vortex_node::{NodeClient, NodeClientConfig};
//! use vortex_node::proto::node_message::Payload;
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! // 无 scheme 时按 TLS 配置有无补 https:// / http://，路径部分自动剥掉
//! let client = NodeClient::connect(
//!     NodeClientConfig::new("192.168.31.97:9001")                 // 默认明文
//!         .with_ca_pem(std::fs::read("node-ca.pem")?)              // 自签 CA（可选）
//!         .with_token("sk-node-token"),                             // 节点鉴权（可选）
//! ).await?;
//!
//! let info = client.node_info().await?;      // NodeInfo
//!
//! // 订阅推送流：连接存续 = 节点在线，断流即离线（由上层重连策略接管）
//! let mut messages = client.subscribe().await?;   // Streaming<NodeMessage>
//! while let Some(msg) = messages.message().await? {
//!     match msg.payload {
//!         Some(Payload::Heartbeat(hb)) => { /* 更新在线状态与负载 */ }
//!         Some(Payload::Notification(n)) => { /* 以消息卡片展示给用户 */ }
//!         Some(Payload::ModelsChanged(m)) => { /* 刷新模型目录（revision 去旧） */ }
//!         None => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};
use tonic::{Code, Request, Status};

use crate::proto::vortex_node_client::VortexNodeClient;
use crate::proto::{NodeInfo, NodeMessage, SubscribeRequest};

/// 客户端配置。[`NodeClientConfig::new`] 起步，链式覆写。
#[derive(Debug, Clone)]
pub struct NodeClientConfig {
    /// `host[:port]` / `http://…` / `https://…`；无 scheme 时按 TLS 配置有无补
    /// `https://` / `http://`；路径部分自动剥离。
    pub endpoint: String,
    /// Bearer 令牌（节点要求鉴权时必须提供）。
    pub token: Option<String>,
    /// TLS 配置；None = 明文。给了则无 scheme 的 endpoint 默认补成 https。
    pub tls: Option<NodeTls>,
    /// 连接超时，默认 10 秒。
    pub connect_timeout: Duration,
    /// 单次一元 RPC 超时，默认 10 秒（不作用于 Subscribe 长流）。
    pub call_timeout: Duration,
    /// HTTP/2 keepalive，默认 30 秒（长连接防 NAT 断链）。
    pub keepalive: Duration,
}

/// TLS 配置。
#[derive(Debug, Clone, Default)]
pub struct NodeTls {
    /// 节点 CA 的 PEM 原文；None 只信任公开链（webpki-roots）。
    pub ca_pem: Option<Vec<u8>>,
    /// 覆写 SNI/校验域名：endpoint 填 IP 而证书是域名（或反之）时需要。
    pub domain: Option<String>,
}

impl NodeTls {
    pub fn ca_pem(mut self, pem: impl AsRef<[u8]>) -> Self {
        self.ca_pem = Some(pem.as_ref().to_vec());
        self
    }

    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }
}

impl NodeClientConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            token: None,
            tls: None,
            connect_timeout: Duration::from_secs(10),
            call_timeout: Duration::from_secs(10),
            keepalive: Duration::from_secs(30),
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// 启用 TLS 并把 PEM 设为自定义根。
    pub fn with_ca_pem(mut self, pem: impl AsRef<[u8]>) -> Self {
        self.tls = Some(NodeTls::default().ca_pem(pem));
        self
    }

    /// 启用 TLS（信任公开链）+ 校验域名覆盖。
    pub fn with_tls(mut self, tls: NodeTls) -> Self {
        self.tls = Some(tls);
        self
    }

    /// 强制明文（已有 TLS 配置时清除）。
    pub fn plaintext(mut self) -> Self {
        self.tls = None;
        self
    }

    pub fn with_connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = d;
        self
    }

    pub fn with_call_timeout(mut self, d: Duration) -> Self {
        self.call_timeout = d;
        self
    }

    pub fn with_keepalive(mut self, d: Duration) -> Self {
        self.keepalive = d;
        self
    }

    /// 规范 endpoint：剥掉 path 与 query，按 TLS 有无补 scheme。
    pub(crate) fn normalized(&self) -> String {
        let ep = self.endpoint.trim().trim_end_matches('/');
        let base = if let Some((scheme, rest)) = ep.split_once("://") {
            format!("{scheme}://{rest}")
        } else if self.tls.is_some() {
            format!("https://{ep}")
        } else {
            format!("http://{ep}")
        };
        let (scheme, rest) = base.split_once("://").expect("已保证含 scheme");
        let host = rest.split(['/', '?']).next().unwrap_or(rest);
        let scheme = scheme.strip_suffix("://").unwrap_or(scheme);
        format!("{scheme}://{host}")
    }
}

/// 已就绪的节点客户端。`Clone` 共享同一 channel，可并发复用。
#[derive(Debug, Clone)]
pub struct NodeClient {
    inner: VortexNodeClient<Channel>,
    token: Option<Arc<str>>,
    call_timeout: Duration,
}

impl NodeClient {
    /// 建立连接；后续 RPC 复用底层 channel。
    pub async fn connect(config: NodeClientConfig) -> Result<Self, NodeClientError> {
        let normalized = config.normalized();
        let endpoint = Endpoint::from_shared(normalized.clone())
            .map_err(|_| NodeClientError::Config(format!("endpoint 无法解析: {normalized}")))?
            .connect_timeout(config.connect_timeout)
            .http2_keep_alive_interval(config.keepalive)
            .tcp_keepalive(Some(config.keepalive));

        let endpoint = if let Some(tls) = &config.tls {
            let mut tls_cfg = ClientTlsConfig::new();
            if let Some(pem) = &tls.ca_pem {
                tls_cfg = tls_cfg.ca_certificate(Certificate::from_pem(pem.clone()));
            }
            if let Some(domain) = &tls.domain {
                tls_cfg = tls_cfg.domain_name(domain.clone());
            }
            endpoint.tls_config(tls_cfg)?
        } else {
            endpoint
        };

        let channel = endpoint.connect().await?;
        let inner = VortexNodeClient::new(channel)
            .max_decoding_message_size(16 * 1024 * 1024)
            .max_encoding_message_size(16 * 1024 * 1024);
        Ok(Self {
            inner,
            token: config.token.map(Arc::from),
            call_timeout: config.call_timeout,
        })
    }

    /// 节点自述（标识与版本）。
    pub async fn node_info(&self) -> Result<NodeInfo, Status> {
        let resp = self
            .inner
            .clone()
            .get_node_info(self.attach(crate::proto::GetNodeInfoRequest {}))
            .await?
            .into_inner();
        Ok(resp)
    }

    /// 订阅节点推送（服务端流）。
    ///
    /// - 流存续 = 节点在线；断流/错误 = 离线，由上层按 [`is_retryable_status`]
    ///   决定重连节奏；
    /// - 三类消息：`Heartbeat`（心跳/状态）、`Notification`（通知）、
    ///   `ModelsChanged`（模型目录变化），前端按类型渲染为消息。
    ///
    /// 注意：长流不应用 [`Self::attach`] 的单次调用超时（否则 300 秒后流会被
    /// 客户端主动掐断），这里只附加鉴权头。
    pub async fn subscribe(&self) -> Result<tonic::Streaming<NodeMessage>, Status> {
        self.inner
            .clone()
            .subscribe(self.authed(SubscribeRequest {}))
            .await
            .map(|r| r.into_inner())
    }

    /// 为一元 RPC 附加鉴权头与超时。
    fn attach<T>(&self, inner: T) -> Request<T> {
        let mut req = self.authed(inner);
        req.set_timeout(self.call_timeout);
        req
    }

    /// 只附加鉴权头（长流用，不设超时）。
    fn authed<T>(&self, inner: T) -> Request<T> {
        let mut req = Request::new(inner);
        if let Some(token) = &self.token {
            match MetadataValue::try_from(format!("bearer {token}").as_str()) {
                Ok(v) => {
                    req.metadata_mut().insert("authorization", v);
                }
                Err(e) => {
                    log::warn!("[node-grpc] 鉴权头无法编码（token 含非 ASCII 字符）: {e}");
                }
            }
        }
        req
    }
}

/// 客户端错误。
#[derive(Debug, thiserror::Error)]
pub enum NodeClientError {
    /// 配置非法（endpoint 解析失败等）。
    #[error("客户端配置错误: {0}")]
    Config(String),
    /// TLS 设置失败（证书解析、域名格式等）。
    #[error("TLS 配置错误: {0}")]
    Tls(#[source] tonic::transport::Error),
    /// 建立连接失败（网络/DNS/拒绝）。
    #[error("连接节点失败: {0}")]
    Connect(#[from] tonic::transport::Error),
}

/// 判断 gRPC 状态是否值得重试/重连（订阅断流后的重连决策沿用该语义）。
pub fn is_retryable_status(status: &Status) -> bool {
    matches!(
        status.code(),
        Code::Unavailable | Code::ResourceExhausted | Code::DeadlineExceeded
    )
}
