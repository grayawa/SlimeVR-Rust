# Windows 修复包 fix3

包含 fix1 的连接修复与 fix2 的重置倒计时音效修复，继续沿用现有配置和系统 WebView2。

## 已有 SteamVR 驱动提示

启动检查发现 SteamVR 安装目录中的 `drivers/slimevr` 目录时，保留现有安装并跳过随包驱动的注册。此前这个保护分支被当成注册失败，前端显示“操作未完成”和英文错误。

现在后端发送独立的普通提示代码，前端通过现有 Fluent 本地化系统显示：

> 使用已有的 SlimeVR 驱动
>
> 检测到已有的 SlimeVR 驱动，已保留现有安装并跳过自动注册。

同一次界面启动中仅显示一次，连接恢复或状态更新不会重复弹出。提示提供简体中文和英文翻译，其他语言沿用原有的英文回退机制。真正的注册失败继续显示错误。

## 验证

真实后端 / WebSocket 回归测试模拟已有驱动安装，确认现有文件保持不变、通知不包含错误、重连仍收到普通提示且服务能响应请求。测试同时检查 Fluent 的中文翻译及英文回退。

TypeScript、ESLint、Rust Clippy、SteamVR 管理测试及 GUI / 桌面 / 后端协议测试通过。Windows GUI 和后端使用 release 交叉编译；打包检查 PE / DLL 依赖、文件 SHA-256 和 ZIP 完整性。本环境未执行 Windows 原生运行。

退出旧版后完整解压，运行 `SlimeVR.exe`。配置与日志位置不变，日志位于 `%APPDATA%\dev.slimevr.SlimeVR\logs\gui-tauri.log`。
