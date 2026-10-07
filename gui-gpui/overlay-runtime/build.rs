fn main() {
    println!("cargo:rerun-if-changed=native/overlay.cpp");
    println!("cargo:rerun-if-changed=../../bindings-provider/openvr/headers/openvr.h");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .cpp(true)
            .cpp_link_stdlib(None)
            .include("../../bindings-provider/openvr/headers")
            .file("native/overlay.cpp")
            .std("c++17")
            .flag_if_supported("-fno-exceptions")
            .flag_if_supported("-fno-rtti")
            .compile("slimevr_overlay_bridge");
    }
}
