# SlimeVR Rust

基于 [SlimeVR/SlimeVR-Server](https://github.com/SlimeVR/SlimeVR-Server) 的 Rust 重写项目。包含 Rust 后端、GPUI Kit 原生前端，以及共用原版 React 界面的 Tauri 宿主。旧 Java/Kotlin 服务与 Gradle 工程已移除；算法对照数据和生成工具仍保留，参考源码从固定历史版本读取。见 [Java 工程清理说明](docs/rust-only-backend.zh-CN.md)。

这是独立开发的衍生项目。上游基线为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`，原作者、许可证及商标说明保留在下文。

## 项目入口

| 目录 | 内容 | 说明 |
| --- | --- | --- |
| `server-rust/` | UDP/HID 接收、姿态算法、校准、AutoBone、SteamVR、OSC/VMC、SolarXR API、YAML 配置、BVH | [后端](server-rust/README.zh-CN.md) · [算法](server-rust/README.core.zh-CN.md) · [API 架构](docs/rust-backend-api-architecture.zh-CN.md) |
| `gui-gpui/` | GPUI Kit 原生界面、组件库、引导、原版 Fluent 翻译 | [构建与使用](gui-gpui/README.zh-CN.md) · [组件库](docs/rust-gpui-components.zh-CN.md) |
| `gui/` | React 界面、Tauri 宿主及打包工具 | [Tauri 构建](gui/README.tauri.md) |
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

GitHub Actions 中的 **SlimeVR AIO** 工作流统一构建 Tauri、GPUI 和 SteamVR Dashboard，支持手动运行和 PR 检查，详见 [CI 说明](docs/rust-ci.zh-CN.md)。硬件、SteamVR/VRChat 实测范围和待验证项以文档为准。

## 文档与许可

- [SteamVR 驱动桥接](docs/rust-steamvr-bridge.zh-CN.md) · [SteamVR 仪表盘 Overlay](docs/rust-steamvr-dashboard.zh-CN.md)
- [原版 YAML 配置兼容](docs/rust-config-compatibility.zh-CN.md) · [BVH 导出](docs/rust-bvh-export.zh-CN.md)
- [前后端联调](docs/rust-frontend-integration.zh-CN.md) · [实机测试清单](docs/rust-unified-hardware-test.zh-CN.md)
- [仓库清理范围](docs/repository-cleanup.zh-CN.md) · [参与开发](CONTRIBUTING.md)

硬件使用和佩戴说明可参考 [SlimeVR 官方文档](https://docs.slimevr.dev/)。本项目构建包从本仓库 GitHub Actions 的 artifacts 下载。

继承的 SlimeVR 代码版权属于 Eiren Rain 和 SlimeVR Contributors，按原始 [MIT](LICENSE-MIT) / [Apache-2.0](LICENSE-APACHE) 双许可证分发。分发源码或二进制时须保留原始许可文件和版权声明；数学代码的第三方许可位于 `server-rust/licenses/`，构建包一并携带。贡献默认使用相同双许可证。

SlimeVR 商标与标识的使用规则见 [TRADEMARK.md](TRADEMARK.md)。本项目不代表 SlimeVR 官方发行版。
