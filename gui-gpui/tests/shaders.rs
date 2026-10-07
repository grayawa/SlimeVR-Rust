#[path = "../vendor/gpui-pre-windows/src/shader_source.rs"]
mod shader_source;

const MODULES: &[&str] = &[
    "quad",
    "shadow",
    "path_rasterization",
    "path_sprite",
    "underline",
    "monochrome_sprite",
    "subpixel_sprite",
    "polychrome_sprite",
    "emoji_rasterization",
];

#[test]
fn portable_sources_embed_includes_and_all_eighteen_entrypoints() {
    for module in MODULES {
        let source = shader_source::embedded_shader_source(*module == "emoji_rasterization");
        assert!(
            !source.contains("#include"),
            "unresolved include in {module}"
        );
        assert!(!source.contains("/home/agent"));
        for stage in ["vertex", "fragment"] {
            assert!(
                source.contains(&format!("{module}_{stage}(")),
                "missing {module}_{stage}"
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_compiles_all_embedded_shaders_without_source_files() {
    use windows::{
        Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile},
        core::PCSTR,
    };
    for module in MODULES {
        let source = shader_source::embedded_shader_source(*module == "emoji_rasterization");
        for (stage, profile) in [("vertex", "vs_4_1\0"), ("fragment", "ps_4_1\0")] {
            let entry = format!("{module}_{stage}\0");
            let mut code = None;
            let mut errors = None;
            let result = unsafe {
                D3DCompile(
                    source.as_ptr().cast(),
                    source.len(),
                    PCSTR::null(),
                    None,
                    None,
                    PCSTR::from_raw(entry.as_ptr()),
                    PCSTR::from_raw(profile.as_ptr()),
                    D3DCOMPILE_OPTIMIZATION_LEVEL3,
                    0,
                    &mut code,
                    Some(&mut errors),
                )
            };
            let details = errors
                .as_ref()
                .map(|blob| unsafe {
                    let bytes = std::slice::from_raw_parts(
                        blob.GetBufferPointer().cast::<u8>(),
                        blob.GetBufferSize(),
                    );
                    String::from_utf8_lossy(bytes).into_owned()
                })
                .unwrap_or_default();
            assert!(result.is_ok(), "{module}_{stage}: {result:?} {details}");
            let code = code.expect("compiler returned no bytecode");
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    code.GetBufferPointer().cast::<u8>(),
                    code.GetBufferSize(),
                )
            };
            assert!(
                bytes.starts_with(b"DXBC"),
                "unexpected bytecode: {module}_{stage}"
            );
        }
    }
}
