//! Compiles `proto/bandcontrol.proto` with a pure Rust protobuf compiler, so no `protoc` is needed.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "../../proto";
    let file = format!("{proto_dir}/bandcontrol.proto");
    println!("cargo:rerun-if-changed={file}");
    let descriptors = protox::compile([file], [proto_dir])?;
    prost_build::Config::new().compile_fds(descriptors)?;
    Ok(())
}
