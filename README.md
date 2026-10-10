> 该项目重度使用 AI，但是正在进行 Code review，且产物已经经过测试。如果您认为这不很妥当，请使用[上游项目](https://github.com/SlimeVR/SlimeVR-Server)。

# SlimeVR-Rust

基于 [SlimeVR/SlimeVR-Server](https://github.com/SlimeVR/SlimeVR-Server) 的 Rust 重写项目。包含 Rust 后端、GPUI Kit 原生前端，以及共用原版 React 界面的 Tauri 宿主。算法测试使用提交到仓库的参考数据，生成工具从固定上游版本读取源码。见 [后端架构](docs/rust-backend-architecture.zh-CN.md) 和 [算法验证](docs/rust-core-validation.zh-CN.md)。

这是独立开发的衍生项目。上游基线为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`，原作者、许可证及商标说明保留在下文。

## 开发状态

**项目处于开发预览阶段（Development Preview），欢迎参与测试与反馈。** 后端、GPUI、Tauri 和 SteamVR Overlay 持续迭代，界面、配置处理和接口可能随版本调整。

自动测试覆盖算法参考数据、协议和状态行为；真实追踪效果、设备组合、CPU 满载表现和平台兼容性通过实机验收记录确认。当前覆盖与测试入口见 [功能状态](docs/rust-feature-status.zh-CN.md) 和 [实机测试清单](docs/rust-unified-hardware-test.zh-CN.md)。

试用前备份 `vrconfig.yml` / `.yaml` 和 GUI 偏好文件，保留可回退的版本。反馈问题请在 [本仓库 Issues](https://github.com/grayawa/SlimeVR-Rust/issues) 中附上构建 commit、系统与固件版本、复现步骤及相关日志；上传前遮蔽 Wi-Fi 凭据、个人路径、设备标识和其他私人信息。

## 项目入口

| 目录           | 内容                                                                                  | 说明                                                                                                                                     |
| -------------- | ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `server-rust/` | UDP/HID 接收、姿态算法、校准、AutoBone、SteamVR、OSC/VMC、SolarXR API、YAML 配置、BVH | [后端](server-rust/README.zh-CN.md) · [算法](server-rust/README.core.zh-CN.md) · [API 架构](docs/rust-backend-api-architecture.zh-CN.md) |
| `gui-gpui/`    | GPUI Kit 原生界面、组件库、引导、原版 Fluent 翻译                                     | [构建与使用](gui-gpui/README.zh-CN.md) · [组件库](gui-gpui/ui/README.md)                                                                 |
| `gui/`         | React 界面、Tauri 宿主及打包工具                                                      | [Tauri 构建](gui/README.tauri.md)                                                                                                        |
| `docs/`        | 架构说明、功能契约、验证数据及实机测试清单                                            | [文档目录](docs/README.md) · [功能状态](docs/rust-feature-status.zh-CN.md)                                                               |

后端直接复用 `vrconfig.yml` / `.yaml`；GUI 偏好沿用原有配置。已有 SlimeVR SteamVR 驱动可继续使用。GPUI 使用原生渲染器；Tauri 在 Windows 使用 WebView2，在 Linux 使用 WebKitGTK。

## 克隆与构建

```sh
git clone --recurse-submodules https://github.com/grayawa/SlimeVR-Rust.git
cd SlimeVR-Rust
# 初始化现有克隆的子模块：
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

Linux x64 用户可从 **Build GPUI** 下载 `SlimeVR-GPUI-Linux-x64`，解压后执行 `./Start-SlimeVR.sh`。完整包以 Ubuntu 24.04 为构建基线，系统依赖与托盘说明见 [原生前端说明](gui-gpui/README.zh-CN.md)。

首次使用通过引导连接、批准及分配设备。启动前退出占用同一接收端口的 Java 服务。完整分发包还需要驱动、OpenVR helper 和运行库，构建脚本及打包方式见各前端说明；生成资源由构建脚本准备并放入分发包。

Tauri 版本在安装 Node.js、pnpm 和系统依赖后构建：

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm tauri:rust:build
```

GitHub Actions 中的 **SlimeVR Checks** 自动检查 PR 和 main；**Build GPUI**、**Build Tauri**、**Build Overlay** 可分别手动构建测试包。**SlimeVR AIO Release** 仅用于发布标签或手动发布打包，详见 [CI 说明](docs/rust-distribution.zh-CN.md)。硬件、SteamVR/VRChat 实测范围和待验证项以文档为准。

## 文档与许可

- [SteamVR 驱动桥接](docs/rust-steamvr-bridge.zh-CN.md) · [SteamVR 仪表盘 Overlay](docs/rust-steamvr-dashboard.zh-CN.md)
- [原版 YAML 配置兼容](docs/rust-config-compatibility.zh-CN.md) · [BVH 导出](docs/rust-bvh-export.zh-CN.md)
- [API 通信契约](docs/rust-backend-api-architecture.zh-CN.md#前端连接与通知) · [实机测试清单](docs/rust-unified-hardware-test.zh-CN.md)
- [文档目录](docs/README.md) · [变更记录](CHANGELOG.md) · [参与开发](CONTRIBUTING.md)
- [维护者发布清单](CONTRIBUTING.md#maintainer-release-checklist)
- [安全问题反馈](SECURITY.md)

硬件使用和佩戴说明可参考 [SlimeVR 官方文档](https://docs.slimevr.dev/)。本项目构建包从本仓库 GitHub Actions 的 artifacts 下载。

本项目维护的新增代码和修改采用 **[GPL-3.0-or-later](LICENSE)**，即 GNU GPL 第 3 版或后续版本。继承的 SlimeVR 代码仍保留 Eiren Rain 和 SlimeVR Contributors 的版权及原始 [MIT](LICENSE-MIT) / [Apache-2.0](LICENSE-APACHE) 许可。新增代码按 GPL 条款授权。第三方代码、字体及图像沿用各自许可证。具体范围、历史版本授权及二进制源码提供方式见 [许可说明](LICENSING.md) 和 [第三方声明](THIRD_PARTY_NOTICES.md)。贡献按所修改代码的许可接受。

SlimeVR 商标与标识的使用规则见 [TRADEMARK.md](TRADEMARK.md)。本项目由独立开发者维护。
