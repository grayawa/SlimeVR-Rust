# SlimeVR 网页界面与 Tauri 运行指南

基于 SlimeVR-Server 提交 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。本次将 React 页面对 Electron 的直接依赖集中到桌面适配层，并添加 Tauri 2 宿主。浏览器、Electron 和 Tauri 使用同一套页面；骨骼解算、AutoBone、追踪器通信可由新的 Rust 后端执行，桌面宿主自动启动 Rust 后端。完整重写与集中验收见 [实施说明](../docs/rust-completion-worklog.zh-CN.md)。

Windows `Tauri-fix6-hand1` 包含手部追踪 / 手柄切换修复，使用与 GPUI 修复版相同的 Rust 后端，并重新构建生产网页及 Tauri 宿主。沿用 fix5 的日志分级、既有 SteamVR 驱动和配置。升级前完全退出旧界面和后端，避免新界面连接旧服务。详见 [手部切换修复说明](../docs/rust-steamvr-hand-handover.zh-CN.md)。

## 目录与边界

```text
gui/
├── src/                       React 页面、状态、SolarXR 客户端
│   ├── platform/
│   │   ├── types.ts           不依赖 Electron 类型的 DesktopAPI
│   │   ├── index.ts           检测浏览器 / Electron / Tauri
│   │   ├── tauri.ts           原生插件与 Rust invoke 的适配
│   │   └── drag.ts            Tauri 窗口拖动
│   └── hooks/desktop.ts       React 桌面上下文
├── public/                    图片、模型、字体、语言文件等静态资源
├── electron/                  原有 Electron 主进程与 preload
├── src-tauri/
│   ├── src/main.rs            原生窗口、托盘、单实例、退出清理
│   ├── src/server.rs          Rust 可执行文件查找、子进程与日志事件
│   ├── src/presence.rs        Discord IPC 与活动更新
│   ├── src/commands.rs        日志、目录、平台、固件请求等命令
│   ├── src/paths.rs           配置与资源路径
│   ├── capabilities/main.json 原生权限
│   ├── tauri.conf.json        默认 GUI 构建
│   └── tauri.rust*.conf.json  打包 Rust 服务可执行文件
└── tests/desktop.test.ts       桌面适配测试
```

```mermaid
flowchart LR
  GUI[React / Vite 页面] --> API[DesktopAPI]
  API --> E[Electron preload]
  API --> T[Tauri 插件 / Rust 命令]
  GUI --> WS[SolarXR WebSocket / FlatBuffers]
  WS --> RUST[Rust SlimeVR Server]
```

React 源码不再导入 Electron 模块或 preload 类型。`window.electronAPI` 只在宿主检测处使用，Tauri 走自己的 IPC；浏览器使用 localStorage。前端与 Rust 服务间默认仍是 `ws://localhost:21110`，没有新增协议或第二套解算实现。

当前仍将两个桌面宿主放在同一个 `gui` 工作区，以共用依赖和保留 Electron 构建。Tauri 的资源、入口和构建流程不需要 Electron 进程。

## 环境准备

- Node.js 与 pnpm **10.33.0**，版本以根目录 `package.json` 为准。
- Rust stable 工具链；安装方式见 [rustup](https://rustup.rs/)。
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)：Windows 需要 Visual Studio C++ Build Tools 与 WebView2；macOS 需要 Xcode Command Line Tools；Linux 需要 GTK 3、WebKitGTK 4.1 和托盘开发库。
- Rust 后端需要 Rust 1.88+；GUI、后端构建与运行无需 JVM。

Debian/Ubuntu 系统依赖示例：

```bash
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

在仓库根目录初始化协议子模块并安装：

```bash
git submodule update --init --recursive
corepack enable
pnpm install --frozen-lockfile
```

`prepare` 会编译 SolarXR TypeScript 协议。若只使用浏览器/Tauri，可在安装依赖前设置 `ELECTRON_SKIP_BINARY_DOWNLOAD=1`，省去 Electron 可执行文件下载；共享工作区中的 Electron 依赖仍保留。已有依赖时不用重复安装。

## Rust 后端与完整桌面包

```bash
# 从仓库根目录执行；构建 Rust 后端、原版 OpenVR helper 并启动 Tauri
pnpm tauri:rust:dev

# 为当前主机操作系统生成安装包
pnpm tauri:rust:build
```

OpenVR helper 构建需要 CMake 3.26+ 和 C++23 编译器；Windows 使用 Visual Studio 2022，Linux 建议 GCC 14+。macOS 不提供 SteamVR 桌面桥接，继续支持浏览器／OSC／VMC 工作流。无需 Java 即可运行 Rust 模式。

更新检查沿用原版的 GitHub release 提示和手动下载安装。Rust/Tauri 构建可设置 `VITE_RELEASE_REPOSITORY=owner/repo`，未设置时关闭该宿主的版本检查，避免引导安装原版 Java 安装包；可用 `VITE_DESKTOP_VERSION` 设置前端比较版本。

## 运行网页或 Tauri

以下命令从仓库根目录执行。

```bash
# 浏览器版本：http://127.0.0.1:5173
pnpm web

# Tauri 开发窗口，自动启动 Vite
pnpm tauri:dev

# 原来的 Electron 开发窗口
pnpm gui
```

GUI 未连接后端服务时会显示连接中或连接失败页面。页面加载成功并不表示已经有追踪数据。

### 连接已有服务

先启动现有 SlimeVR Server，再启动 Tauri。默认宿主发现本机 `21110` 端口已占用时跳过后端启动；这是端口检查，不会验证占用进程的身份。也可以明确只运行 GUI：

```bash
pnpm --dir gui exec tauri dev -- -- --no-server
```

Tauri CLI 在第二个 `--` 后才将参数传给应用。直接调用 GUI 子命令时，先确保执行过 `pnpm update-solarxr`，`pnpm install` 已经包含这一步。

浏览器可用 `http://127.0.0.1:5173/?ip=192.168.1.10&port=21110` 指定远程服务，参数沿用原来的 WebSocket 客户端逻辑。远程连接需要自行保证服务可达。

### 由 Tauri 启动 Rust 服务

默认查找资源目录、可执行文件旁的 Rust 后端；开发构建也查找 `server-rust/target/release` 和 `debug`。显式参数优先：

```bash
pnpm --dir gui exec tauri dev -- -- --rust-server /absolute/path/slimevr-server
```

| 参数 | 说明 |
| --- | --- |
| `--no-server` | 只连接已有服务 |
| `--backend auto` / `rust` | 自动发现后端 / 要求找到 Rust 后端 |
| `--rust-server <file>` | 指定 Rust 可执行文件 |
| `--path <directory>` | 指定 Rust 后端所在目录 |
| `--config <file>` | 指定原版 SlimeVR YAML |
| `--log-level <level>` | error / warn / info / debug / trace |
| `--steam` / `-s` | 启用 GUI 的 Steam 模式标识 |

宿主转发后端 stdout / stderr、启动失败和退出事件，并保存有限的历史。退出时关闭自己启动的进程的 stdin，让后端完成 BVH / journal 写入，超时后终止。已经在其他地方运行的服务由原启动方式管理。

## 打包

```bash
# 完整 Rust 桌面包：后端、网页、OpenVR helper 和 SteamVR 驱动
pnpm tauri:rust:build

# 仅打包 GUI，连接单独启动的后端
pnpm tauri:build
```

输出在 `gui/src-tauri/target/release/`，安装包在 `bundle/`。请在目标系统构建。完整 Rust 包使用平台对应的 `tauri.rust*.conf.json`；不再打包 Java JAR 或 JRE。

## 功能与持久化

| 功能                                    | 浏览器           | Electron     | Tauri                  |
| --------------------------------------- | ---------------- | ------------ | ---------------------- |
| React 页面、SolarXR 数据、3D 预览       | 支持             | 支持         | 支持                   |
| 设置与缓存                              | localStorage     | 原有文件存储 | Tauri store 文件       |
| 最小化、最大化、关闭、窗口拖动          | 无桌面窗口接口   | 支持         | 支持                   |
| 托盘显示/隐藏/退出                      | 无               | 原有行为     | 支持                   |
| 文件/目录选择、BVH 保存位置             | 受浏览器能力限制 | 支持         | 原生对话框             |
| Rust 服务子进程与日志                   | 手动运行服务     | 原有行为     | 支持                   |
| 外链打开、配置/日志目录、固件元数据请求 | 受浏览器能力限制 | 支持         | 支持                   |
| Discord Rich Presence                   | 无               | 支持         | 支持 Discord IPC，含更新／禁用／重连 |

Tauri 托盘菜单暂时使用英文固定标签。窗口大小/位置由 window-state 插件保存。GUI 设置和缓存分别为 `gui-settings.dat`、`gui-cache.dat`，保留原有文件名、JSON 数据格式和应用标识 `dev.slimevr.SlimeVR`：

| 系统    | Tauri GUI 数据目录                                              | 后端配置目录                                            |
| ------- | --------------------------------------------------------------- | ------------------------------------------------------------ |
| Windows | `%APPDATA%/dev.slimevr.SlimeVR`                                 | `%APPDATA%/dev.slimevr.SlimeVR`                              |
| macOS   | `~/Library/Application Support/dev.slimevr.SlimeVR`             | 同左                                                         |
| Linux   | `$XDG_DATA_HOME/dev.slimevr.SlimeVR`，默认 `~/.local/share/...` | `$XDG_CONFIG_HOME/dev.slimevr.SlimeVR`，默认 `~/.config/...` |

路径以 Tauri 系统目录 API 和 原版兼容的后端配置规则为准。GUI 数据目录与原 Electron 的 `getGuiDataFolder()` 对齐，可沿用已有 GUI 设置；不要同时用两个宿主写同一份文件。窗口位置改由 Tauri 插件管理，不自动迁移 Electron 的窗口状态。GUI 日志位于 GUI 数据目录的 `logs/gui-tauri.log`；“打开配置文件夹”指向 后端配置目录，`override.ftl` 也从该目录加载。

另修复了文件选择始终使用目录模式、取消 BVH 保存仍发送录制请求的问题，以及桌面分支中条件调用 React hooks 的问题。

## 验证

```bash
pnpm --dir gui lint
pnpm --dir gui test:desktop
pnpm --dir gui web:build
pnpm --dir gui build
cd gui/src-tauri
cargo fmt --check
cargo test
```

适配测试使用 Tauri 官方 IPC mock，覆盖浏览器回退、Electron 桥接复用、Tauri 检测、原生文件/目录选择及取消、设置/缓存隔离与保存、异步事件监听清理。Rust 测试覆盖启动选项、移除的 Java 启动参数和固件资源 URL 范围；Node 测试覆盖 Rust 可执行文件发现与 YAML 选择。

分配页连接回归测试：先编译 Rust 后端，再运行 `pnpm --dir gui test:assignment-browser`。该命令构建生产网页，随后自动启动测试后端和网页预览，使用 Chromium / Chrome / Edge 检查实际页面导航、设置更新与断线恢复。浏览器由系统提供；可通过 `SLIMEVR_BROWSER_EXECUTABLE` 指定路径，通过 `SLIMEVR_RUST_BINARY` 指定后端。未检测到浏览器时测试会跳过，请确认测试结果包含通过项。

本次验证记录（2026-10-03）：

- TypeScript、ESLint、Prettier 检查通过；5 项桌面适配测试和 2 项 Rust 测试通过。
- Vite 网页、原 Electron 构建以及 Linux `tauri build --debug --no-bundle` 通过。
- Chromium 加载网页无未捕获异常；Linux Tauri 在 Xvfb 中实际显示连接失败页和欢迎页，设置/缓存文件与 GUI 原生日志正常生成。
- Tauri 连接本地测试 WebSocket，发出 SolarXR 二进制消息；该测试没有模拟完整 SlimeVR 服务响应。
- Windows GNU 目标的 `cargo check --target x86_64-pc-windows-gnu` 通过；Windows/macOS 原生运行及安装包未在本环境验证。

追踪器、VR 运行时和固件刷写仍需连接相应设备做端到端测试。

## Rust 后端

现在可执行仓库根目录的 `pnpm tauri:rust`，由 Tauri 自动启动 Rust 后端。配置保存、支持的 SolarXR 操作与各平台资源打包方式见 [前后端联调说明](../docs/rust-frontend-integration.zh-CN.md)。默认 `auto` 查找 Rust 后端，旧 Java 工程与启动入口的清理见 [说明](../docs/rust-only-backend.zh-CN.md)。

Rust 直接读取并写回原版 `vrconfig.yml` / `.yaml`，默认位于应用的服务目录。可用 `--config <路径>` 指向已有配置；字段和迁移范围见 [配置兼容说明](../docs/rust-config-compatibility.zh-CN.md)。

Linux/Windows 的 Rust 后端默认启动 SteamVR 协议 2 桥，复用已有 OpenVR Driver。辅助程序和参数见 [SteamVR 桥接说明](../docs/rust-steamvr-bridge.zh-CN.md)。

## SteamVR 驱动资源

`pnpm tauri:dev`、`pnpm tauri:build` 和根目录 `pnpm tauri:rust` 会准备官方 v6.0.0 的 Windows x64／Linux x64、aarch64 驱动资源，包含固定下载哈希和许可证。需要 Python 3，首次下载需网络。直接调用 `tauri dev/build` 前先执行 `pnpm --dir gui tauri:drivers`。Rust 自动查找 SteamVR 并注册未安装驱动，现有注册保持不变；前端沿用原启用流程。资源不含 Bindings Provider，仍使用已有安装或 `--bindings-provider` 指定路径。

日常 AutoBone 文件、敲击分配、磁力计控制、驱动管理和实测边界见 [实施说明](../docs/rust-daily-workflow.zh-CN.md)。

## 日志分级

默认 `info`，支持 `--log-level debug` / `trace` 和环境变量 `SLIMEVR_LOG_LEVEL`。同一级别用于前端、原生宿主和它启动的 Rust 后端；日志过滤在序列化和 IPC 前执行。`logs/gui-tauri.log` 每约 10 MiB 轮转，最多保留四份历史文件。便携包提供 `Start-SlimeVR-Debug.cmd`，启动前需要完全退出已有实例。日志含真实级别、来源及毫秒时间，进程状态只落盘一次。详细级别、命令行兼容和收集步骤见 [日志说明](../docs/rust-logging.zh-CN.md)。
