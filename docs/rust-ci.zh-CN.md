# SlimeVR 检查、独立构建与发布

日常检查与应用构建分开。PR 和 main 的代码修改自动运行 **SlimeVR Checks**。需要测试包时，在 Actions 选择对应的独立构建，点击 **Run workflow** 并选择分支。

| 工作流                    | 触发方式                       | 产物与范围                                                       |
| ------------------------- | ------------------------------ | ---------------------------------------------------------------- |
| **SlimeVR Checks**        | PR、main 代码修改，或手动      | Linux / Windows 后端和无窗口 GPUI 检查、React 检查、网页生产资源 |
| **Build Windows Backend** | 手动，或被应用工作流调用       | Rust 后端 release EXE、许可与源码通知；供单独运行或应用打包      |
| **Build GPUI**            | 手动                           | GPUI 桌面、组件预览、探针和后端的 Windows / Linux x64 包         |
| **Build Tauri**           | 手动                           | Tauri 完整 Windows 解压包和安装包；可额外选择其他平台            |
| **Build Overlay**         | 手动                           | SteamVR 仪表盘附加包；连接已运行的后端                           |
| **SlimeVR AIO Release**   | 推送 `v*` 标签，或手动发布打包 | 检查、三个独立 Windows 应用包及 AIO 合集                         |

独立应用构建完成后上传自己的 artifact。AIO 只为发布使用：检查与构建并行，后端编译一次，GPUI 与 Tauri 下载同一份后端 EXE 后并行构建；Overlay 独立构建。合集 job 等待检查和三个应用构建成功，再下载同一次运行的包，核对 SHA-256 与 ZIP CRC 并合并。

`windows-app.yml` 提供独立构建和 AIO 共用的 Windows 任务；`linux-gpui.yml` 提供 Linux GPUI 任务。手动入口为对应的 Build 工作流。驱动、OpenVR helper 和微软运行库由构建脚本准备及校验。工作流产物上传到 artifacts，GitHub Release 由维护者发布。

## 下载

打开对应的成功运行，在 **Artifacts** 中选择：

| Artifact                           | 内容                                                                        |
| ---------------------------------- | --------------------------------------------------------------------------- |
| `SlimeVR-GPUI-Linux-x64`           | Linux 原生界面、组件预览、Rust 后端、驱动、OpenVR helper 与许可证（tar.gz） |
| `SlimeVR-GPUI-Windows-x64`         | GPUI、组件预览、Rust 后端、驱动、OpenVR helper、运行库与许可证              |
| `SlimeVR-Tauri-Windows-x64`        | Tauri、Rust 后端、驱动、OpenVR helper、运行库与许可证；使用系统 WebView2    |
| `SlimeVR-Overlay-Windows-x64`      | 仪表盘附加程序、OpenVR DLL 和运行库；连接已有后端                           |
| `SlimeVR-AIO-Windows-x64`          | 上述三个独立 ZIP、校验文件和记录提交的合集 manifest                         |
| `slimevr-tauri-windows-installers` | Tauri 安装包、项目许可与构建源码记录                                        |
| `gui-dist`                         | 用于网页部署 / 调试的生产资源                                               |

先解压下载的 artifact ZIP，再解开里面的应用包：Windows 使用 `.zip`，Linux 使用 `.tar.gz`。GPUI 或 Tauri 二选一启动桌面服务；Overlay 单独解压，在桌面程序和 SteamVR 启动后运行。下载保留 30 天。

**Build Tauri** 手动运行时勾选 `other_tauri_platforms`，会另外构建 Linux x64、Linux ARM64 和 macOS Tauri 包。AIO 发布合集面向 Windows。GPUI 使用原生渲染器，Tauri 在 Windows 使用 WebView2，在 Linux 使用 WebKitGTK。

## 检查范围

**SlimeVR Checks** 中工作流／打包工具、React、Linux Rust 和 Windows Rust 并行检查。工作流检查使用固定版本及 SHA-256 的 actionlint。网页检查执行类型、ESLint、Prettier、桌面适配和 WebSocket 测试及生产构建。Rust 检查执行后端测试与 Clippy、GPUI 无窗口状态／通信／音效／帧节流测试与 Clippy、Overlay 输入测试、真实后端回环；Linux Rust job 复用刚编译的后端执行 React 的真实通信测试。格式检查包含 Tauri 源码。

完整 GPUI / Overlay 原生 feature 的 Clippy 与单元测试随各自的 Windows 构建执行；GPUI 包还运行真实后端联调。Linux GPUI job 在 Ubuntu 24.04 构建并检查原生 desktop feature，执行真实后端回环、专用 D-Bus 会话里的托盘生命周期 / 菜单测试，以及 Xvfb + 软件 Vulkan 的窗口渲染截图。Linux tar.gz 保留执行权限，核对 ELF 架构、系统动态库、许可证、源码提交和文件 SHA-256；运行截图与日志上传为 `SlimeVR-GPUI-Linux-smoke`。Tauri 构建执行 Windows React 通信测试与 Tauri 原生命令测试。日常 PR 检查覆盖后端、无窗口 GPUI 与 React；修改 GPUI 渲染器或 Tauri 原生宿主时另外运行对应应用构建。各包继续校验着色器、PE / DLL 依赖、许可来源、ZIP CRC 和 SHA-256。

后端、GPUI / Overlay、Tauri 和日常检查使用各自的 Rust 缓存，Tauri 与 React 复用 pnpm 缓存。首次构建仍需要下载和编译依赖，拆分主要缩短等待目标应用的时间。同一个 PR 或 main 分支的新提交会取消旧检查；手动及发布构建保留独立的运行生命周期。

真实设备、VRChat、重置音效、头显显示及 CPU 满载表现按 [实机清单](rust-unified-hardware-test.zh-CN.md) 验证。
