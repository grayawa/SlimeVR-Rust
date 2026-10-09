# Portable Windows shader compilation

Upstream: crates.io `gpui-pre-windows` 0.3.8, Apache-2.0, Zed snapshot
`279fe070bb389b79652e52065b2f001edcc0b11b`. The upstream source and license are
retained here for reproducible builds from this vendor directory.

The optional `runtime-shaders` feature embeds both HLSL programs and their shared
alpha correction include, then uses `D3DCompile` on the in-memory source. It skips
the build-time `fxc.exe` step. Release builds compile optimized shaders and keep
GPUI's graphics debug assertions disabled.

Portable builds use the embedded shader source with `D3DCompile`. Release mode
keeps graphics debug assertions disabled. Packaging validates this loader and
the embedded shader programs.

Changed files: `Cargo.toml`, `build.rs`, `src/gpui_windows.rs`,
`src/directx_renderer.rs`; added `src/shader_source.rs`. All other files retain
the upstream content in the original shader patch.

## Optional SteamVR dashboard output

The `overlay-output` feature adds a UI-thread registration in
`src/overlay_output.rs`, virtual visibility in `src/window.rs`, and an opt-in
branch in `src/directx_renderer.rs`. Registered windows copy into a stable shared
D3D11 texture and submit it through the caller's sink, through a GPU copy path.
GPU recovery discards the export texture; resizing reallocates it. Hidden hosts
receive asynchronous frame requests from the dashboard event loop. Unregistered
desktop windows retain their original renderer behavior.

`src/hidden_window.rs` applies the requested bounds with `SetWindowPos` while
keeping the host hidden and inactive. This delivers the initial `WM_SIZE` needed
to resize DirectX's initial 1x1 target. The client size initializes the export texture before the first submission.
A Windows regression test checks hidden client dimensions and visibility.

`src/directx_devices.rs` uses the optional compositor adapter selected before
application construction. Dashboard builds select the compositor GPU for texture export. Normal desktop
builds use the existing adapter selection.
