//! Vortex 远程节点 gRPC 协议 v1 与 tonic 客户端（极简版）。
//!
//! - 协议定义：`proto/vortex_node.proto`（`vortex.node.v1`），本 crate build 时生成
//!   client/server 双侧代码。仅两个 RPC：`GetNodeInfo`（节点自述）与
//!   `Subscribe`（服务端流，推送心跳/通知/模型目录变化）。
//! - 高层客户端：[`client::NodeClient`]——vortex 网关调用远程节点时使用。
//!   封装了 TLS（含自定根 CA）、Bearer 鉴权与长连接 keepalive。
//!   **在线判定**：订阅流存续即在线，断流即离线。
//!
//! 服务端（远程节点侧）实现参考：`tests/integration.rs` 里有完整的 tonic server
//! 骨架，可直接复制到节点服务工程。

#![allow(clippy::large_enum_variant)]

/// 生成代码（`vortex.node.v1`）。经 `pub use` 暴露稳定路径，
/// 避免 include 展开处的模块前缀在 IDE 中难以导航。
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/vortex.node.v1.rs"));
}

pub mod client;

pub use client::{NodeClient, NodeClientConfig, NodeClientError};
pub use proto::{vortex_node_client::VortexNodeClient, vortex_node_server::VortexNodeServer};
