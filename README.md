# SlimeVR Rust

基于 [SlimeVR/SlimeVR-Server](https://github.com/SlimeVR/SlimeVR-Server) 的 Rust 重写项目。包含 Rust 后端、GPUI Kit 原生前端，以及共用原版 React 界面的 Tauri 前端。原 Java/Kotlin 和 Electron 代码保留，便于行为对照和兼容性验证。

这是独立开发的衍生项目。上游基线为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`，原作者、许可证及商标说明保留在下文。

## 项目入口

| 目录 | 内容 | 说明 |
| --- | --- | --- |
| `server-rust/` | UDP/HID 接收、姿态算法、校准、AutoBone、SteamVR、OSC/VMC、SolarXR API、YAML 配置、BVH | [后端](server-rust/README.zh-CN.md) · [算法](server-rust/README.core.zh-CN.md) · [API 架构](docs/rust-backend-api-architecture.zh-CN.md) |
| `gui-gpui/` | GPUI Kit 原生界面、组件库、引导、原版 Fluent 翻译 | [构建与使用](gui-gpui/README.zh-CN.md) · [组件库](docs/rust-gpui-components.zh-CN.md) |
| `gui/` | React 界面、Electron/Tauri 宿主及打包工具 | [Tauri 构建](gui/README.tauri.md) |
| `docs/` | 移植说明、功能对照、修复记录及实机测试清单 | [交接记录](docs/rust-completion-worklog.zh-CN.md) · [统一测试清单](docs/rust-unified-hardware-test.zh-CN.md) |

后端直接复用 `vrconfig.yml` / `.yaml`；GUI 偏好沿用原有配置。已有 SlimeVR SteamVR 驱动可继续使用。GPUI 前端不需要 WebView2；Tauri 前端需要系统提供 WebView2。

## 克隆与构建

```sh
git clone --recurse-submodules https://github.com/grayawa/SlimeVR-Rust.git
cd SlimeVR-Rust
# 已经克隆过但没有初始化子模块时：
git submodule update --init --recursive
```

Rust 后端需要 Rust 1.88+；GPUI 前端需要 Rust 1.92+。Windows 本机构建建议安装 Visual Studio C++ Build Tools、Windows SDK 和 CMake；其他平台的系统依赖见对应目录说明。

```sh
cargo build --manifest-path server-rust/Cargo.toml --release --locked
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
cargo build --manifest-path gui-gpui/Cargo.toml --release --locked --bin slimevr-gpui
```

Windows 上可让原生界面启动刚构建的后端：

```powershell
.\gui-gpui\target\release\slimevr-gpui.exe --backend .\server-rust\target\release\slimevr-server.exe
```

首次使用通过引导连接、批准及分配设备。启动前退出占用同一接收端口的 Java 服务。完整分发包还需要驱动、OpenVR helper 和运行库，构建脚本及打包方式见各前端说明；这些生成资源不放入 Git。

Tauri 版本在安装 Node.js、pnpm 和系统依赖后构建：

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm tauri:rust:build
```

GitHub Actions 中的 `Rust and Tauri test bundles` 和 `GPUI native frontend` 支持手动构建。硬件、SteamVR/VRChat 实测范围和待验证项以文档为准。

## 上游项目说明

以下保留上游 SlimeVR 项目介绍及许可说明；其中官方安装器安装的是上游版本。

Server app for SlimeVR ecosystem

Server orchestrates communication between multiple sensors and integrations, like SteamVR.

Sensors implementations:
* [SlimeVR Tracker for ESP](https://github.com/SlimeVR/SlimeVR-Tracker-ESP) - ESP microcontrollers and multiple IMUs are supported
* [owoTrack Mobile App](https://github.com/abb128/owoTrackVRSyncMobile) - use phones as trackers (limited functionality and compatibility)
* [SlimeVR Wrangler](https://github.com/carl-anders/slimevr-wrangler) - use Nintendo Switch Joycon controllers as trackers

Integrations:
* Use [SlimeVR OpenVR Driver](https://github.com/SlimeVR/SlimeVR-OpenVR-Driver) as a driver for SteamVR.
* Use built-in OSC Trackers support for FBT integration with VRChat, PCVR or Standalone.
* Use built-in VMC support for sending and receiving tracking data to and from other apps such as VSeeFace.
* Export recordings as .BVH files to integrate motion capture data into 3d applications such as Blender.

## Installing
It's highly recommended to install using the installer downloadable here: https://github.com/SlimeVR/SlimeVR-Installer/releases/latest/download/slimevr_web_installer.exe

Latest setup instructions are [in our docs](https://docs.slimevr.dev/server/index.html).

## Building & Contributing
For information on building and contributing to the codebase, see [CONTRIBUTING.md](CONTRIBUTING.md).

The shared web interface can also run in Tauri. See [the Tauri setup and build guide](gui/README.tauri.md).

A native Rust interface using GPUI Kit is available in [gui-gpui](gui-gpui/README.zh-CN.md). See [its feature and validation checklist](docs/rust-gpui-functional-parity.zh-CN.md) for the Windows test package scope.

The backend migration plan and its original sequencing are documented in [the Rust rewrite plan](docs/rust-backend-rewrite-plan.zh-CN.md).

The first Rust UDP receiver is runnable independently. See [its setup, recording and validation guide](server-rust/README.zh-CN.md).

The Rust pose core supports live UDP solving, full-body skeletons, constraints, leg corrections, alignment, localization, and offline AutoBone. See [the algorithm core guide](server-rust/README.core.zh-CN.md).

## Translating

Translation is done via Pontoon at [i18n.slimevr.dev](https://i18n.slimevr.dev/). Please join our [Discord translation forum](https://discord.com/channels/817184208525983775/1050413434249949235) to coordinate.

## License clarification
**SlimeVR software** (including server, firmware, drivers, installer, documents, and others - see
licence for each case specifically) **is distributed under a dual MIT/Apache 2.0 License
([LICENSE-MIT] and [LICENSE-APACHE]). The software is the copyright of the SlimeVR
contributors.**

**However, these licenses have some limits, and if you wish to distribute software based
on SlimeVR, you need to be aware of them:**

* When distributing any software that uses or is based on SlimeVR, you have to provide
  to the end-user at least one of the original, unmodified [LICENSE-MIT] or
  [LICENSE-APACHE] files from SlimeVR. This includes the `Copyright (c) 2020 Eiren Rain
  and SlimeVR Contributors` part of the license. It is insufficient to use a generic MIT
  or Apache-2.0 License, **it must be the original license file**.
* This applies even if you distribute software without the source code. In this case,
  one way to provide it to the end-user is to have a menu in your application that lists
  all the open source licenses used, including SlimeVR's.

Please refer to the [LICENSE-MIT] and [LICENSE-APACHE] files if you are at any point
uncertain what the exact requirements are.

## Trademark and Logo use
**SlimeVR is a trademark or a registered trademark of SlimeVR B.V. Usage of SlimeVR software, hardware, or other intellectual property in this or other repositories does not grant you the right to use SlimeVR trademark as your own.**

For more information, please refer to the [TRADEMARK].

## Contributions
Any contributions submitted for inclusion in this repository will be dual-licensed under
either:

- MIT License ([LICENSE-MIT])
- Apache License, Version 2.0 ([LICENSE-APACHE])

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.

You also certify that the code you have used is compatible with those licenses or is
authored by you. If you're doing so on your work time, you certify that your employer is
okay with this and that you are authorized to provide the above licenses.

[LICENSE-MIT]: LICENSE-MIT
[LICENSE-APACHE]: LICENSE-APACHE
[TRADEMARK]: TRADEMARK.md


*if you read this, u cute*

Rust backend + Web/Tauri integration: [联调与启动说明](docs/rust-frontend-integration.zh-CN.md).

Rust BVH recording: [导出规则与使用说明](docs/rust-bvh-export.zh-CN.md).

Rust backend configuration: [原版 vrconfig.yml 复用与兼容范围](docs/rust-config-compatibility.zh-CN.md).

Rust SteamVR integration: [驱动桥接、共享设置与验证范围](docs/rust-steamvr-bridge.zh-CN.md).

Rust daily workflow: [AutoBone 文件、敲击分配、磁力计与驱动管理，以及集中实测清单](docs/rust-daily-workflow.zh-CN.md).

Rust / Tauri completion: [实施交接与构建](docs/rust-completion-worklog.zh-CN.md) · [统一实机测试清单](docs/rust-unified-hardware-test.zh-CN.md).
