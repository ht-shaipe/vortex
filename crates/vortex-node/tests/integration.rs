//! 集成测试：标准 tonic 服务端骨架 + 客户端全链路往返。
//!
//! # 远程节点服务端实现参考
//!
//! 服务端工程只需依赖本 crate，实现 `vortex_node_server::VortexNode`
//! trait（测试内的 [`VortexNodeStub`] 即是可直接复制的骨架），再启动：
//!
//! ```ignore
//! use vortex_node::VortexNodeServer;
//! tonic::transport::Server::builder()
//!     .add_service(VortexNodeServer::new(MyNode::default()))
//!     .serve("0.0.0.0:9001".parse()?)
//!     .await?;
//! ```

use std::time::Duration;

use futures::Stream;
use std::pin::Pin;
use tonic::transport::Server as TonicServer;
use tonic::{Request, Response, Status};

use vortex_node::proto::node_message::Payload;
use vortex_node::proto::vortex_node_server::VortexNode;
use vortex_node::proto::{
    GetNodeInfoRequest, Heartbeat, ModelEntry, ModelsChanged, NodeMessage, NodeInfo,
    Notification, NotificationLevel, SubscribeRequest,
};

/// 最小可用节点桩：自述 + 三类消息各推一条后结束流。
#[derive(Default)]
struct VortexNodeStub;

type SubscribeStream = Pin<Box<dyn Stream<Item = Result<NodeMessage, Status>> + Send>>;

#[tonic::async_trait]
impl VortexNode for VortexNodeStub {
    type SubscribeStream = SubscribeStream;

    async fn get_node_info(
        &self,
        _request: Request<GetNodeInfoRequest>,
    ) -> Result<Response<NodeInfo>, Status> {
        Ok(Response::new(NodeInfo {
            node_id: "stub-node-1".into(),
            node_name: "测试节点".into(),
            software_version: "test".into(),
        }))
    }

    async fn subscribe(
        &self,
        _request: Request<SubscribeRequest>,
    ) -> Result<Response<SubscribeStream>, Status> {
        let now_ms = chrono_like_ms();
        let messages: Vec<Result<NodeMessage, Status>> = vec![
            Ok(NodeMessage {
                message_id: "m-1".into(),
                timestamp_ms: now_ms,
                payload: Some(Payload::Heartbeat(Heartbeat {
                    ok: true,
                    in_flight: 2,
                })),
            }),
            Ok(NodeMessage {
                message_id: "m-2".into(),
                timestamp_ms: now_ms,
                payload: Some(Payload::Notification(Notification {
                    level: NotificationLevel::Warning as i32,
                    title: "配额即将耗尽".into(),
                    body: "剩余 5%".into(),
                    kind: "quota".into(),
                })),
            }),
            Ok(NodeMessage {
                message_id: "m-3".into(),
                timestamp_ms: now_ms,
                payload: Some(Payload::ModelsChanged(ModelsChanged {
                    models: vec![ModelEntry {
                        id: "stub-model".into(),
                        display_name: "Stub Model".into(),
                    }],
                    revision: 1,
                })),
            }),
        ];
        Ok(Response::new(Box::pin(futures::stream::iter(messages)) as SubscribeStream))
    }
}

/// 毫秒时间戳（测试桩用，避免给 crate 引入 chrono 依赖）。
fn chrono_like_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

async fn spawn_server(addr: std::net::SocketAddr) {
    tokio::spawn(async move {
        TonicServer::builder()
            .add_service(vortex_node::VortexNodeServer::new(VortexNodeStub))
            .serve(addr)
            .await
            .expect("测试服务器启动失败");
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
}

#[tokio::test]
async fn roundtrip_info_and_subscribe_messages() {
    // 0 端口让 OS 分配
    let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    // 拿真实端口：先绑一次只读 listener 获取端口再释放（测试便利做法）
    let probe = std::net::TcpListener::bind(addr).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    spawn_server(addr).await;

    let client = vortex_node::NodeClient::connect(
        vortex_node::NodeClientConfig::new(addr.to_string())
            .with_call_timeout(Duration::from_secs(10)),
    )
    .await
    .expect("连接失败");

    let info = client.node_info().await.expect("node_info 失败");
    assert_eq!(info.node_id, "stub-node-1");
    assert_eq!(info.node_name, "测试节点");

    // 订阅推送流：三类消息各一条，随后服务端结束流
    let mut stream = client.subscribe().await.expect("subscribe 失败");
    let mut seen_heartbeat = false;
    let mut seen_notification = false;
    let mut seen_models = false;
    let mut count = 0;
    loop {
        let Some(msg) = stream.message().await.expect("流读取失败") else {
            break; // 服务端结束流
        };
        count += 1;
        assert!(!msg.message_id.is_empty());
        assert!(msg.timestamp_ms > 0);
        match msg.payload {
            Some(Payload::Heartbeat(hb)) => {
                assert!(hb.ok);
                assert_eq!(hb.in_flight, 2);
                seen_heartbeat = true;
            }
            Some(Payload::Notification(n)) => {
                assert_eq!(n.level, NotificationLevel::Warning as i32);
                assert_eq!(n.title, "配额即将耗尽");
                assert_eq!(n.kind, "quota");
                seen_notification = true;
            }
            Some(Payload::ModelsChanged(m)) => {
                assert_eq!(m.revision, 1);
                assert_eq!(m.models.len(), 1);
                assert_eq!(m.models[0].id, "stub-model");
                seen_models = true;
            }
            None => panic!("空消息"),
        }
    }
    assert_eq!(count, 3);
    assert!(seen_heartbeat && seen_notification && seen_models);
}

#[test]
fn notification_level_enum_defaults() {
    // 保证生成代码对枚举/oneof 字段的正确暴露与默认值语义
    assert_eq!(NotificationLevel::Unspecified as i32, 0);
    assert_eq!(NotificationLevel::Info as i32, 1);
    assert_eq!(NotificationLevel::Warning as i32, 2);
    assert_eq!(NotificationLevel::Error as i32, 3);
    let msg = NodeMessage::default();
    assert!(msg.message_id.is_empty());
    assert!(msg.payload.is_none());
}
