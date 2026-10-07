# SlimeVR AIO 构建

仓库只保留 `.github/workflows/aio.yml`，Actions 中名称为 **SlimeVR AIO**。推送 main 和修改代码的 PR 自动检查、构建；也可手动点击 **Run workflow**。旧的网页、Tauri、GPUI、Dashboard 四个独立 workflow 已整合。

Windows job 使用同一份源码和 Rust 后端，构建 GPUI 桌面、组件预览、探针、Overlay 与 Tauri，并生成完整解压包。GPUI 的几个可执行文件使用同一组 feature 一次构建；驱动、OpenVR helper 和微软运行库通过已有脚本准备，不依赖旧压缩包。

## 下载

成功运行后在 **Artifacts** 中选择：

| Artifact | 内容 |
| --- | --- |
| `SlimeVR-GPUI-Windows-x64` | GPUI、组件预览、Rust 后端、驱动、OpenVR helper、运行库与许可证 |
| `SlimeVR-Tauri-Windows-x64` | Tauri、Rust 后端、驱动、OpenVR helper、运行库与许可证；使用系统 WebView2 |
| `SlimeVR-Overlay-Windows-x64` | 仪表盘附加程序、OpenVR DLL 和运行库；连接已有后端 |
| `SlimeVR-AIO-Windows-x64` | 上述三个独立 ZIP、校验文件和记录提交的合集 manifest |
| `slimevr-tauri-windows-installers` | Tauri 安装包 |
| `gui-dist` | 用于网页部署 / 调试的生产资源 |

解压下载的 artifact ZIP，再解压里面需要使用的应用 ZIP。GPUI 或 Tauri 二选一启动桌面服务；Overlay 单独解压，在桌面程序和 SteamVR 启动后运行。下载保留 30 天。

手动运行时勾选 `other_tauri_platforms`，会另外构建 Linux x64、Linux ARM64 和 macOS Tauri 包；默认只构建 Windows 和网页。GPUI 桌面包不需要 WebView2，Tauri 不附 WebView2 Runtime。

## 检查范围

网页 job 校验固定版本及 SHA-256 的 actionlint，执行类型、ESLint、Prettier、桌面适配测试及生产网页构建。

Windows job 执行后端测试与 Clippy、参考源码工具测试、GPUI 通信 / 状态 / 重置音效 / 帧节流测试、Overlay 输入测试、React 通信测试、真实后端回环测试、Tauri 原生命令测试。打包脚本校验着色器、PE / DLL 依赖、许可来源、ZIP CRC 和 SHA-256；合集打包再次核对各应用校验文件。

首次构建需要编译两套桌面工具链，Windows 超时设为 120 分钟，后续运行复用 Rust 和 pnpm 缓存。同一个 PR 或 main 分支的新提交会取消旧构建；手动运行互不取消。

CI 不执行真实设备或 VRChat 实测，重置音效、头显显示及 CPU 满载表现仍按 [实机清单](rust-unified-hardware-test.zh-CN.md) 验证。
