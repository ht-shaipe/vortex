use std::time::Duration;

use tokio_stream::Stream;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{Request, Streaming};

use crate::channel::channel_service_client::ChannelServiceClient;
use crate::channel::{ClientMessage, ServerMessage};

/// 默认连接超时。
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 默认 TCP keepalive 间隔。
pub const DEFAULT_TCP_KEEPALIVE: Duration = Duration::from_secs(30);
/// 默认 HTTP/2 keepalive 间隔。
pub const DEFAULT_HTTP2_KEEPALIVE: Duration = Duration::from_secs(30);
/// 默认 HTTP/2 keepalive 超时。
pub const DEFAULT_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);

/// gRPC 终端长连接配置。
#[derive(Debug, Clone)]
pub struct ChannelClientConfig {
    /// gRPC 地址，例如 `https://channel.example.com`。
    ///
    /// 本地直连时使用 `http://127.0.0.1:19999`。
    pub endpoint: String,
    /// 登录接口返回的访问 token。
    ///
    /// 以 `authorization: <token>`（原样，不带 Bearer 前缀）随 Open RPC 上行，
    /// 服务端经 `ChannelBackend::verify_connect` 校验（默认放行，业务接入后生效）。
    /// 传空字符串时不会发送 `authorization`。
    pub token: String,
    pub connect_timeout: Duration,
    pub tcp_keepalive: Duration,
    pub http2_keepalive_interval: Duration,
    pub keepalive_timeout: Duration,
}

impl ChannelClientConfig {
    pub fn new(endpoint: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            token: token.into(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            tcp_keepalive: DEFAULT_TCP_KEEPALIVE,
            http2_keepalive_interval: DEFAULT_HTTP2_KEEPALIVE,
            keepalive_timeout: DEFAULT_KEEPALIVE_TIMEOUT,
        }
    }
}

/// gRPC 客户端。
///
/// 只负责建立 channel 和打开 `channel.v1.ChannelService/Open` 双向流；
/// 心跳、重连、消息分发与上层事件桥接由调用方负责。
pub struct ChannelClient {
    inner: ChannelServiceClient<Channel>,
    token: String,
}

impl ChannelClient {
    /// 建立到 gRPC 服务的 HTTP/2 channel。https 端点自动启用 TLS（系统根证书）。
    pub async fn connect(config: ChannelClientConfig) -> Result<Self, ChannelError> {
        let endpoint = Endpoint::from_shared(config.endpoint)
            .map_err(|e| ChannelError::InvalidEndpoint(e.to_string()))?
            .connect_timeout(config.connect_timeout)
            .tcp_keepalive(Some(config.tcp_keepalive))
            .http2_keep_alive_interval(config.http2_keepalive_interval)
            .keep_alive_timeout(config.keepalive_timeout)
            .keep_alive_while_idle(true);

        // tonic 不会按 scheme 自动启用 TLS：https 端点必须显式配置，
        // 否则连接时报 transport error（根因 HttpsUriWithoutTlsSupport）。
        let endpoint = if endpoint.uri().scheme_str() == Some("https") {
            endpoint
                .tls_config(ClientTlsConfig::new().with_enabled_roots())
                .map_err(|e| ChannelError::TlsConfig(e.to_string()))?
        } else {
            endpoint
        };

        let channel = endpoint.connect().await?;
        Ok(Self {
            inner: ChannelServiceClient::new(channel),
            token: config.token,
        })
    }

    /// 打开双向流。`outbound` 由调用方通过 mpsc channel 驱动。
    ///
    /// 非空 token 会原样写入 gRPC metadata 的 `authorization: <token>`
    /// （不带 Bearer 前缀，由业务侧定义凭证格式）。
    pub async fn open<S>(&mut self, outbound: S) -> Result<Streaming<ServerMessage>, ChannelError>
    where
        S: Stream<Item = ClientMessage> + Send + 'static,
    {
        let mut request = Request::new(outbound);
        if !self.token.is_empty() {
            let token = MetadataValue::try_from(self.token.clone())
                .map_err(|e| ChannelError::InvalidAuthMetadata(e.to_string()))?;
            request.metadata_mut().insert("authorization", token);
        }

        let response = self.inner.open(request).await?;
        Ok(response.into_inner())
    }

    /// 取出底层生成的 tonic client，便于自定义调用。
    pub fn into_inner(self) -> ChannelServiceClient<Channel> {
        self.inner
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("gRPC 地址无效: {0}")]
    InvalidEndpoint(String),
    #[error("鉴权 metadata 无效: {0}")]
    InvalidAuthMetadata(String),
    #[error("TLS 配置无效: {0}")]
    TlsConfig(String),
    #[error("连接 gRPC 服务失败: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gRPC 调用失败: {0}")]
    Rpc(#[from] tonic::Status),
}
