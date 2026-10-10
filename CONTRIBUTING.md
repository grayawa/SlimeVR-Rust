# Contributing to SlimeVR Rust

This fork uses a Rust backend, a GPUI native frontend, and a shared React interface with a Tauri host. Production code and reference tools are organized by their current responsibilities. See [README.md](README.md) for project entry points and [CI checks](docs/rust-distribution.zh-CN.md) for pull request validation.

## Prerequisites

- Git with recursive submodules.
- Rust 1.88+ for the backend; Rust 1.92+ for GPUI.
- Node.js from `.node-version` and pnpm from `package.json` for the React frontend.
- CMake and a C++23 compiler for OpenVR helpers. Windows builds require Visual Studio C++ Build Tools and the Windows SDK; Tauri additionally uses WebView2.
- Platform dependencies listed in [the GPUI guide](gui-gpui/README.zh-CN.md) and [the Tauri guide](gui/README.tauri.md).

## Build and check

```sh
git submodule update --init --recursive
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
cargo clippy --manifest-path server-rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path gui-gpui/Cargo.toml --no-default-features --locked
cargo build --manifest-path gui-gpui/Cargo.toml --release --locked --bin slimevr-gpui
```

For the shared React GUI:

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm --dir gui lint
pnpm --dir gui test:desktop
pnpm --dir gui test:backend
pnpm tauri:rust:dev
pnpm tauri:rust:build
```

Backend communication tests use `server-rust/target/debug/slimevr-server`; build it first or set `SLIMEVR_RUST_BINARY` to an existing executable.

`pnpm gui` starts the Tauri development window with the Rust backend. Use `pnpm tauri:rust:build` for a complete desktop build, or `pnpm web` for browser development with a separately running backend.

Run `cargo fmt` inside each Rust workspace directory. From the repository root, format the backend with `cargo fmt --manifest-path server-rust/Cargo.toml -p slimevr-core -p slimevr-server`. React uses ESLint and Prettier. Keep behavior changes covered by relevant tests; real SteamVR / tracker validation is documented separately.

## Documentation and comments

Describe the current implementation and contract directly: what a value means,
what an operation does, its inputs, outputs, limits and error conditions. Prefer
concrete, affirmative wording. Keep each explanation focused on its subject.

When interfaces, behavior, parameters or scope change, rewrite the affected
passages and comments together with the code. Replace outdated semantics with
the final contract. Use migration history, compatibility changes or deprecation
rationale when the task specifically calls for them. Preserve exact license
texts, copyright notices and third-party attribution.

## Upstream behavior references

Ordinary Rust tests use committed golden fixtures with the Rust toolchain. To regenerate upstream references, see [reference source handling](docs/rust-core-validation.zh-CN.md#参考源码与-fixtures). Test-only Kotlin adapters are kept separately from production code.

## SolarXR Protocol

SolarXR is the WebSocket / FlatBuffers protocol shared by the backend and GUI. Update its schema in the `solarxr-protocol` submodule and regenerate the needed language bindings using its upstream scripts and documented `flatc` version. Commit protocol changes in the submodule and then update the submodule reference here.

## Licensing and upstream contributions

By submitting contributions to project-owned code, you agree to license them under **GPL-3.0-or-later**, unless explicitly agreed otherwise. Changes in separately licensed vendor/submodule code retain its applicable license.

Preserve the original MIT / Apache-2.0 license files, copyright notices and all applicable third-party licenses. See [LICENSING.md](LICENSING.md), [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [TRADEMARK.md](TRADEMARK.md). GPL-only changes need an additional compatible grant from the relevant rights holders before incorporation into permissively licensed SlimeVR upstream.

When submitting changes to SlimeVR upstream, follow [its contribution policies](https://github.com/SlimeVR/.github/blob/main/profile/CONTRIBUTING.md) and repository instructions.
