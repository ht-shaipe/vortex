//! 终端 gRPC 通道冒烟客户端。
//!
//! 对指定服务端做一次真实的「连接 → Hello 握手 → 心跳」全流程验证，
//! 全部通过输出 PASS，任一环节失败输出 FAIL 并以非零码退出。
//!
//! 用法:
//!
//! ```bash
//! # 本地直连(默认 127.0.0.1:19999)
//! cargo run -p channel-client --example smoke
//!
//! # 指定地址与设备指纹
//! cargo run -p channel-client --example smoke -- http://10.0.0.5:19999 dev-001
//!
//! # 经 Nginx 的对外 HTTPS 链路
//! cargo run -p channel-client --example smoke -- https://channel.example.com dev-001
//! ```

use std::time::Duration;

use channel_client::channel::{ClientMessage, Heartbeat, Hello, client_message, server_message};
use channel_client::{ChannelClient, ChannelClientConfig};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let endpoint = args
        .next()
        .unwrap_or_else(|| "http://127.0.0.1:19999".to_string());
    let device_id = args.next().unwrap_or_else(|| "smoke-device-1".to_string());

    println!("[smoke] 目标: {endpoint}  设备: {device_id}");
    println!("[smoke] 建立 HTTP/2 连接 ...");
    let mut client = ChannelClient::connect(ChannelClientConfig::new(&endpoint, "")).await?;
    println!("[smoke] HTTP/2 连接已建立");

    let (tx, rx) = mpsc::channel::<ClientMessage>(16);
    let mut inbound = client.open(ReceiverStream::new(rx)).await?;
    println!("[smoke] Open 双向流已建立");

    // 1. Hello 握手: 首帧必须为成功的 HelloAck
    tx.send(ClientMessage {
        payload: Some(client_message::Payload::Hello(Hello {
            device_id: device_id.clone(),
            app_key: "smoke-app".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            device_info: None,
            user_info: None,
        })),
    })
    .await?;

    let ack = match tokio::time::timeout(Duration::from_secs(5), inbound.next()).await {
        Ok(Some(Ok(m))) => match m.payload {
            Some(server_message::Payload::HelloAck(ack)) => ack,
            Some(other) => fail(format!("首条下行消息不是 HelloAck: {other:?}")),
            None => fail("首条下行消息缺少 payload"),
        },
        Ok(Some(Err(status))) => fail(format!("握手阶段收到错误: {status}")),
        Ok(None) => fail("连接在握手阶段断开"),
        Err(_) => fail("5s 内未收到 HelloAck（服务端未响应握手）"),
    };
    if !ack.success {
        fail(format!("握手被拒绝: {}", ack.message));
    }
    println!(
        "[smoke] HelloAck ✓  server_time={}  补发任务={}个",
        ack.server_time,
        ack.pending_tasks.len()
    );

    // 2. 心跳: 每次发送后 5s 内应收到 HeartbeatAck
    for seq in 1..=3 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send(ClientMessage {
            payload: Some(client_message::Payload::Heartbeat(Heartbeat { seq })),
        })
        .await?;

        let reply = match tokio::time::timeout(Duration::from_secs(5), inbound.next()).await {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(status))) => fail(format!("第 {seq} 次心跳收到错误: {status}")),
            Ok(None) => fail("连接在心跳阶段被服务端断开"),
            Err(_) => fail(format!("第 {seq} 次心跳 5s 内未收到 HeartbeatAck")),
        };
        match reply.payload {
            Some(server_message::Payload::HeartbeatAck(_)) => {
                println!("[smoke] HeartbeatAck ✓  seq={seq}")
            }
            other => fail(format!("第 {seq} 次心跳收到意外消息: {other:?}")),
        }
    }

    println!("\n[smoke] PASS — gRPC 通道双向可用 (device_id={device_id})");
    Ok(())
}

/// 打印失败原因并以非零码退出。
fn fail<E: std::fmt::Display>(msg: E) -> ! {
    eprintln!("\n[smoke] FAIL — {msg}");
    std::process::exit(1);
}
