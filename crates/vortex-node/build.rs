//! 生成 tonic/prost 代码（协议定义在 `proto/vortex_node.proto`）。
//!
//! 需要 `protoc` 在 PATH 中（Homebrew 即可）。产出 `<OUT_DIR>/vortex.node.v1.rs`，
//! 同时包含 client（VortexNodeClient）与 server（VortexNodeServer）两侧代码：
//! - vortex 网关用 client；
//! - 远程节点服务端可直接复用 crate 里的 server 生成代码实现。

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile_protos(&["proto/vortex_node.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/vortex_node.proto");
    Ok(())
}
