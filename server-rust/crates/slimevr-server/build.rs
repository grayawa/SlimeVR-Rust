fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/ProtobufMessages.proto");
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.file_descriptor_set_path(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("steamvr-descriptor.pb"),
    );
    config.compile_protos(&["proto/ProtobufMessages.proto"], &["proto"])?;
    Ok(())
}
