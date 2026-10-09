# Windows 解压包

Windows x64 桌面包分为 GPUI 与 Tauri，两者共用 Rust 后端、原版配置和 SteamVR 驱动。GPUI 使用原生渲染器，Tauri 使用系统 WebView2 Runtime。Actions 构建与下载见 [CI](rust-ci.zh-CN.md)。

## 启动

解压整个应用目录到固定位置，完全退出已有前端与占用同一端口的后端，再双击 `SlimeVR.exe` 或 `Start-SlimeVR.cmd`。桌面程序启动随包 Rust 后端，也可按参数连接已有服务。SteamVR 注册使用解压目录中的驱动路径，注册后保持目录位置。

Tauri 包使用系统 WebView2。Windows 10 用户可从 [微软 WebView2 Runtime 下载页](https://developer.microsoft.com/microsoft-edge/webview2/) 安装 Evergreen Runtime。若包中带 `WebView2Loader.dll`，保留该加载库；MSVC 构建采用静态加载库。

完整包包含界面、后端、固定版本 SlimeVR 驱动、Bindings Provider、OpenVR DLL、所需 VC++ DLL、使用说明、许可、源码版本与文件校验信息。运行依赖由包或系统提供，开发工具用于源码构建。

## 配置与日志

配置沿用 `%APPDATA%/dev.slimevr.SlimeVR/vrconfig.yml` / `.yaml`。GUI 偏好和日志使用同一应用数据目录下的各自文件。切换构建包时复用已有配置；指定路径的方法见各前端 README。

诊断入口为 `Start-SlimeVR-Debug.cmd`。先完全退出已有实例，再调试启动并复现，压缩整个 `logs` 目录，附上时间与步骤。详见 [日志说明](rust-logging.zh-CN.md)。

## 本地打包

Tauri 使用 `gui/scripts/package-windows-portable.py`，GPUI 使用 `gui-gpui/scripts/package-windows.py`，仪表盘使用 `gui-gpui/scripts/package-overlay-windows.py`。参数见各脚本 `--help`。输入为当前构建的 EXE、平台 helper、驱动与固定来源的运行库，输出 ZIP 和 SHA-256。

直接用 Cargo 构建 Tauri 生产宿主时启用 `--features tauri/custom-protocol`。打包检查网页资源、PE / DLL、shader、许可、ZIP CRC 和文件哈希；Windows 窗口、设备、SteamVR 与声音按 [实机清单](rust-unified-hardware-test.zh-CN.md) 验收。
