# 构建、下载与分发

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

## Windows 解压运行

解压整个应用目录到固定位置，完全退出已有前端与占用同一端口的后端，再双击 `SlimeVR.exe` 或 `Start-SlimeVR.cmd`。桌面程序启动随包 Rust 后端，也可按参数连接已有服务。SteamVR 注册使用解压目录中的驱动路径，注册后保持目录位置。

Tauri 包使用系统 WebView2。Windows 10 用户可从 [微软 WebView2 Runtime 下载页](https://developer.microsoft.com/microsoft-edge/webview2/) 安装 Evergreen Runtime。若包中带 `WebView2Loader.dll`，保留该加载库；MSVC 构建采用静态加载库。

完整包包含界面、后端、固定版本 SlimeVR 驱动、Bindings Provider、OpenVR DLL、所需 VC++ DLL、使用说明、许可、源码版本与文件校验信息。运行依赖由包或系统提供，开发工具用于源码构建。

### 配置与日志

配置沿用 `%APPDATA%/dev.slimevr.SlimeVR/vrconfig.yml` / `.yaml`。GUI 偏好和日志使用同一应用数据目录下的各自文件。切换构建包时复用已有配置；指定路径的方法见各前端 README。

诊断入口为 `Start-SlimeVR-Debug.cmd`。先完全退出已有实例，再调试启动并复现，压缩整个 `logs` 目录，附上时间与步骤。详见 [日志说明](rust-logging.zh-CN.md)。

## Linux GPUI 解压运行

完整包以 Ubuntu 24.04 为构建基线。解开外层 artifact ZIP 和应用 tar.gz 后，在固定目录执行 `./Start-SlimeVR.sh`。程序会启动随包 Rust 后端，`--attach` 可连接已有服务。

系统提供 Vulkan 驱动、ALSA、Fontconfig、X11 / Wayland 和桌面 D-Bus 会话；托盘使用 StatusNotifier，GNOME 可启用 AppIndicator 扩展。USB / HID 权限规则、平台构建依赖及托盘关闭行为见 [GPUI README](../gui-gpui/README.zh-CN.md)。真实 SteamVR / VRChat 与 Wayland 按实机清单验收。

## 本地打包

Tauri 使用 `gui/scripts/package-windows-portable.py`，GPUI 使用 `gui-gpui/scripts/package-windows.py`，仪表盘使用 `gui-gpui/scripts/package-overlay-windows.py`。参数见各脚本 `--help`。输入为当前构建的 EXE、平台 helper、驱动与固定来源的运行库，输出 ZIP 和 SHA-256。

直接用 Cargo 构建 Tauri 生产宿主时启用 `--features tauri/custom-protocol`。打包检查网页资源、PE / DLL、shader、许可、ZIP CRC 和文件哈希；Windows 窗口、设备、SteamVR 与声音按 [实机清单](rust-unified-hardware-test.zh-CN.md) 验收。

Linux GPUI 使用 `gui-gpui/scripts/package-linux.py`，打包检查 ELF 架构、动态库、执行权限、许可证和文件哈希。

完整桌面和 Overlay 解压包携带 `README.md`、`CHANGELOG.md` 与 `docs/`。包内指南互相使用相对链接；工作流包中的源码与其余开发说明链接到构建 commit，便于对照实际版本。本地标签构建的文档链接使用仓库 main，实际构建输入由分发者按 `SOURCE-CODE.txt` 提供。
