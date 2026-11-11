fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/channel.proto");
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/channel.proto"], &["proto"])?;
    Ok(())
}
