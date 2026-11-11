//! 应用通道客户端。
//!
//! 提供 `ChannelClient` 封装，建立到服务端的 gRPC 双向长连接。
//! protobuf 类型由 build.rs（tonic-prost-build）从 crate 内 `proto/channel.proto` 生成。

/// protobuf 包 `channel.v1` 生成的 client 代码。
pub mod channel {
    tonic::include_proto!("channel.v1");
}

pub mod client;

pub use client::{ChannelClient, ChannelClientConfig, ChannelError};

/// 当前协议包名，用于日志和协议检查。
pub const PROTOCOL_PACKAGE: &str = "channel.v1";

/// 终端双向流 RPC 全路径。
pub const CHANNEL_PATH: &str = "/channel.v1.ChannelService/Open";

#[cfg(test)]
mod tests {
    use crate::channel::{
        ClientMessage, DeviceInfo, Hello, PushMessage, ServerMessage, TaskAck, TaskAssign,
        client_message::Payload as ClientPayload, server_message::Payload as ServerPayload,
    };
    use prost::Message;

    #[test]
    fn hello_message_roundtrip() {
        let message = ClientMessage {
            payload: Some(ClientPayload::Hello(Hello {
                device_id: "device-1".into(),
                app_key: "demo-app".into(),
                version: "0.1.0".into(),
                device_info: Some(DeviceInfo {
                    device_brand: "Apple".into(),
                    device_model: "Mac16,8".into(),
                    device_type: "laptop".into(),
                    screen_width: 2560,
                    screen_height: 1440,
                    os_name: "macos".into(),
                    os_version: "27.0.1".into(),
                    wifi_enabled: true,
                    bluetooth_enabled: false,
                    host_version: "0.1.5".into(),
                }),
                user_info: None,
            })),
        };

        let encoded = message.encode_to_vec();
        let decoded = ClientMessage::decode(encoded.as_slice()).expect("decode hello message");

        match decoded.payload {
            Some(ClientPayload::Hello(hello)) => {
                assert_eq!(hello.device_id, "device-1");
                assert_eq!(hello.app_key, "demo-app");
                let info = hello.device_info.expect("device_info present");
                assert_eq!(info.device_model, "Mac16,8");
                assert_eq!(info.screen_width, 2560);
                assert!(info.wifi_enabled);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn task_assign_and_ack_roundtrip() {
        let assign = ServerMessage {
            payload: Some(ServerPayload::TaskAssign(TaskAssign {
                task_id: 1001,
                task_type: 1,
                attempt: 1,
                payload: r#"{"version":"0.2.0"}"#.into(),
            })),
        };
        let decoded =
            ServerMessage::decode(assign.encode_to_vec().as_slice()).expect("decode task assign");
        match decoded.payload {
            Some(ServerPayload::TaskAssign(task)) => {
                assert_eq!(task.task_id, 1001);
                assert_eq!(task.task_type, 1);
            }
            other => panic!("unexpected payload: {other:?}"),
        }

        let ack = ClientMessage {
            payload: Some(ClientPayload::TaskAck(TaskAck {
                task_id: 1001,
                attempt: 1,
                success: true,
                message: String::new(),
            })),
        };
        let decoded =
            ClientMessage::decode(ack.encode_to_vec().as_slice()).expect("decode task ack");
        match decoded.payload {
            Some(ClientPayload::TaskAck(ack)) => {
                assert_eq!(ack.task_id, 1001);
                assert!(ack.success);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn push_message_roundtrip() {
        let message = ServerMessage {
            payload: Some(ServerPayload::Push(PushMessage {
                message_id: "push-1".into(),
                message_type: "notify".into(),
                payload: r#"{"text":"hello"}"#.into(),
                server_time: 1_700_000_000,
            })),
        };

        let decoded =
            ServerMessage::decode(message.encode_to_vec().as_slice()).expect("decode push message");
        match decoded.payload {
            Some(ServerPayload::Push(push)) => {
                assert_eq!(push.message_id, "push-1");
                assert_eq!(push.message_type, "notify");
                assert_eq!(push.payload, r#"{"text":"hello"}"#);
                assert_eq!(push.server_time, 1_700_000_000);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }
}
