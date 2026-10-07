# Portable Windows shader compilation

Upstream: crates.io `gpui-pre-windows` 0.3.8, Apache-2.0, Zed snapshot
`279fe070bb389b79652e52065b2f001edcc0b11b`. The upstream source and license are
retained here so the patch is reproducible and does not modify Cargo's registry cache.

The optional `runtime-shaders` feature embeds both HLSL programs and their shared
alpha correction include, then uses `D3DCompile` on the in-memory source. It skips
the build-time `fxc.exe` step. Release builds compile optimized shaders and keep
GPUI's graphics debug assertions disabled.

Upstream's debug compiler uses `D3DCompileFromFile` with `CARGO_MANIFEST_DIR`.
That directory only exists on the build host. Enabling debug assertions in the
previous cross-build did not make the application portable, and caused the first
Windows package to panic during DirectWrite initialization with OS error 3.

Changed files: `Cargo.toml`, `build.rs`, `src/gpui_windows.rs`,
`src/directx_renderer.rs`; added `src/shader_source.rs`. All other files retain
the upstream content.
