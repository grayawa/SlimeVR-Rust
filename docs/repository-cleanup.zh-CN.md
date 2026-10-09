# 仓库内容与开发入口

项目包含 Rust 后端、GPUI 原生界面、React / Tauri 界面和 SteamVR Dashboard Overlay。生产后端与参考数据的使用方法见 [后端与参考工具](rust-only-backend.zh-CN.md)。

## 源码与资源

| 范围           | 内容                                                                            |
| -------------- | ------------------------------------------------------------------------------- |
| 后端           | UDP / HID 接收、算法、校准、AutoBone、SteamVR、OSC / VMC、SolarXR、YAML 与 BVH  |
| 桌面与 VR 界面 | GPUI、React / Tauri、Overlay、引导、Fluent 翻译、主题、模型、字体与音效         |
| 平台集成       | SolarXR 与 OpenVR 子模块、Bindings Provider、驱动下载与平台资源构建脚本         |
| 用户数据       | 原版 YAML、GUI 偏好、设备固件更新与 Discord Rich Presence；偏好保存保留未知字段 |
| 自动验证       | 算法 golden fixtures、参考生成工具、协议测试、回环测试与打包检查                |
| 许可           | 项目 GPL 条款、上游版权与商标说明、数学代码和分发资源的第三方许可               |

网页工具链中的 `electron-to-chromium` 为 browserslist 提供浏览器版本映射数据。桌面入口使用 GPUI 或 Tauri；网页开发使用 React。

## 开发入口

后端：`cargo build --manifest-path server-rust/Cargo.toml --release --locked`。

GPUI：参见 [原生界面说明](../gui-gpui/README.zh-CN.md)。Tauri：使用 `pnpm gui` 或 `pnpm tauri:rust:build`，参见 [Tauri 指南](../gui/README.tauri.md)。浏览器开发使用 `pnpm web`，并单独运行后端。

PR 检查由 **SlimeVR Checks** 执行；GPUI、Tauri 和 Overlay 各有独立构建入口，AIO 用于发布合集。具体触发方式、产物和验证范围见 [CI 说明](rust-ci.zh-CN.md)。本项目构建包通过本仓库的 Actions artifacts 分发，设备固件通过固件服务更新。本地日志和轮转用于问题排查。
