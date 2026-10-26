# vortex-node — 远程节点 gRPC 协议 v1（极简版）

Vortex 网关（客户端） ↔ 远程节点（服务端） 之间的 gRPC 协议与 tonic 客户端。

**只做两件事**：让 Vortex 知道哪些设备在线；让设备（后端）把少量类型化消息推给
Vortex 前端，以消息卡片形式展示给用户。

```
┌─────────────┐  gRPC (tonic)    ┌──────────────────┐
│ Vortex 网关  │ ───────────────► │   远程节点         │
│ (本 crate    │  GetNodeInfo     │  (服务端，另行实现) │
│  = 客户端)   │  Subscribe 流 ◄──│  推: 心跳/通知/目录 │
└─────────────┘                  └──────────────────┘
```

## 组成

| 文件 | 说明 |
|------|------|
| `proto/vortex_node.proto` | 协议定义（`vortex.node.v1`），字段级中文注释即接口文档 |
| `src/client.rs` | 高层客户端 `NodeClient`：TLS（自定根 CA）/Bearer 鉴权/长连接 keepalive |
| `tests/integration.rs` | 端到端往返测试 + **标准 tonic 服务端骨架（服务端实现可直接复制）** |

## 协议内容速览

`VortexNode` 服务共 2 个 RPC：

| RPC | 类型 | 用途 |
|-----|------|------|
| `GetNodeInfo` | 一元 | 节点自述：`node_id` / `node_name` / `software_version` |
| `Subscribe` | **服务端流** | 节点推送通道，三类消息（见下），流存续 = 节点在线 |

推送消息（`NodeMessage`，含 `message_id` + `timestamp_ms` + oneof payload）：

| 类型 | 字段 | 前端展示 |
|------|------|----------|
| `Heartbeat` | `ok` / `in_flight` | 在线状态与负载（10~30s 一帧） |
| `Notification` | `level`(info/warning/error) / `title` / `body` / `kind` | 消息卡片，按 `kind` 路由 |
| `ModelsChanged` | `models[]` / `revision` | 刷新设备模型列表（revision 去旧） |

关键设计：

- **在线判定 = 订阅流状态**：流保持连接即在线；断流（含重连失败）即离线。
  不需要独立的 HealthCheck RPC。
- **推送为主**：设备侧变化（告警、目录变更）通过 Subscribe 流主动推送，无需轮询。
- **鉴权**：HTTP 头 `authorization: bearer <token>`（沿用 gRPC 惯例）。
- **字段策略**：只增不改不删，破坏性变更升版本包（`vortex.node.v2`）。

## 客户端（vortex 侧，已实现）

```rust
use vortex_node::{NodeClient, NodeClientConfig};
use vortex_node::proto::node_message::Payload;

let client = NodeClient::connect(
    NodeClientConfig::new("192.168.31.97:9001")      // 无 scheme 默认明文；https 需配 CA
        .with_ca_pem(std::fs::read("node-ca.pem")?)  // 自签根 CA（可选）
        .with_token("sk-node-token"),                // 节点鉴权（可选）
).await?;

let info = client.node_info().await?;                // 标识与版本

let mut messages = client.subscribe().await?;        // 长流（不设调用超时）
while let Some(msg) = messages.message().await? {
    match msg.payload {
        Some(Payload::Heartbeat(hb)) => { /* 在线 + 负载 */ }
        Some(Payload::Notification(n)) => { /* 消息卡片 */ }
        Some(Payload::ModelsChanged(m)) => { /* 刷新目录 */ }
        None => {}
    }
}
// 流断开 = 节点离线，按 is_retryable_status 决定重连节奏
```

## 服务端实现指引（待你实现）

1. 新工程依赖本 crate：
   ```toml
   [dependencies]
   vortex-node = { path = "../vortex/crates/vortex-node" }
   ```
2. 实现 trait（生成代码自带；骨架见 `tests/integration.rs::VortexNodeStub`）：
   ```rust
   #[tonic::async_trait]
   impl vortex_node::proto::vortex_node_server::VortexNode for MyNode {
       type SubscribeStream = Pin<Box<dyn Stream<Item = Result<NodeMessage, Status>> + Send>>;
       // 两个方法即可；subscribe 返回推送流（心跳定时 + 事件触发）
   }
   ```
3. 启动并叠加 TLS/鉴权：
   ```rust
   tonic::transport::Server::builder()
       .add_service(vortex_node::VortexNodeServer::new(MyNode::default()))
       .serve("0.0.0.0:9001".parse()?)
       .await?;
   ```
4. 构建本 crate 需要 `protoc`（protobuf 编译器，Homebrew `brew install protobuf`）。

## 测试

```bash
cargo test -p vortex-node   # 往返集成测试 + 枚举语义测试
```
