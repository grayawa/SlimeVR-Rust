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
the upstream content in the original shader patch.

## Optional SteamVR dashboard output

The `overlay-output` feature adds a UI-thread registration in
`src/overlay_output.rs`, virtual visibility in `src/window.rs`, and an opt-in
branch in `src/directx_renderer.rs`. Registered windows copy into a stable shared
D3D11 texture and submit it through the caller's sink, without CPU readback.
GPU recovery discards the export texture; resizing reallocates it. Hidden hosts
receive asynchronous frame requests from the dashboard event loop. Unregistered
desktop windows retain their original renderer behavior.

`src/directx_devices.rs` uses the optional compositor adapter selected before
application construction. The adapter preference is unused for normal desktop
builds and avoids exporting a texture from a different GPU on multi-GPU systems.
