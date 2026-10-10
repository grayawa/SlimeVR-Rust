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

## Maintainer release checklist

Use `SlimeVR-Rust` as the product name and identify the desktop, tray and dashboard as an independent development preview. Tauri uses application identifier `io.github.grayawa.slimevr-rust`; preferences and backend configuration use the existing `dev.slimevr.SlimeVR` directories. Follow [TRADEMARK.md](TRADEMARK.md) for inherited marks.

Before publishing source or attachments:

- [ ] Verify the README's AI disclosure, preview status, backup advice and feedback / validation links.
- [ ] Scan the Git history, discussion text and workflow logs being published; review secret-rule candidates.
- [ ] Review artifacts, screenshots, attachments and reports for credentials, device identifiers and personal paths.
- [ ] Verify recursive submodules, lockfiles and build / test instructions.

For each binary release:

- [ ] Fix the build commit, submodules, lockfiles and tool versions; provide local changes with the corresponding source.
- [ ] Check bundled license files and source against [LICENSING.md](LICENSING.md) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
- [ ] Verify `SOURCE-CODE.txt`, `BUILD-MANIFEST.json` or `BUILD-SOURCE.json` against the actual build.
- [ ] Check installer notice resources and the notices shipped with installer artifacts.
- [ ] Check hashes, archive integrity, architectures, runtime libraries and platform dependencies.
- [ ] Record passed / failed / untested hardware checks with the system, devices, configuration and log times; use the [hardware checklist](docs/rust-unified-hardware-test.zh-CN.md).
- [ ] Publish with a development-preview or GitHub prerelease label; retain binaries, checksums and complete corresponding-source / build instructions in Releases.

Packaging scripts carry notices and source references. Tauri development / build hooks stage these resources for installers. GitHub source archives are used with recursive-submodule instructions. Actions artifacts have a 30-day retention period; Releases provide long-term downloads. Build entry points are in the [distribution guide](docs/rust-distribution.zh-CN.md).

Review repository settings for secret scanning, push protection and private vulnerability reporting; `main` rulesets and required checks; Actions token permissions and external-contributor approvals; and the repository description, topics and Issue templates. Workflows use explicit `contents: read` and PR checks use `pull_request`. Security reports follow [SECURITY.md](SECURITY.md). Historical publication audits are in [CHANGELOG.md](CHANGELOG.md).
