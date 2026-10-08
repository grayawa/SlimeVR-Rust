# Rust 仓库清理范围

项目现在保留 Rust 后端、GPUI 原生界面、React / Tauri 界面和 SteamVR Dashboard Overlay。Java / Gradle 的移除及算法参考源码处理见 [后端清理说明](rust-only-backend.zh-CN.md)。

## 已删除

- Electron 主进程、preload、资源、打包配置、专用脚本与测试，及仅供 Electron 使用的 Node 依赖。根目录 `pnpm gui` 和 `pnpm build` 改为 Tauri 的完整 Rust 开发 / 构建入口。
- 上游 Sentry SDK、构建上传、指标采集、错误收集同意页与设置项。界面启动不再执行远程遥测初始化；本地日志、日志分级和问题排查入口仍在。
- 指向原版服务 release 的软件更新弹窗和 GPUI 更新入口；本项目当前从自己的 GitHub Actions artifacts 获取构建。此项不影响追踪器固件升级。
- 上游 Crowdin / Pontoon 同步、rebase 和 update-manifest 工作流、对应配置、上游维护者 CODEOWNERS 与捐助配置。翻译文件本身保留。
- 为旧工程和 Electron 打包服务的 Nix / direnv 环境、未使用的 PWA 安装资源、Android 包装器专用分支，以及确认未引用的图片。

Electron 可执行文件、运行时和打包器不再安装。锁文件中的 `electron-to-chromium` 是 browserslist 的版本映射数据，属于网页工具链的传递依赖，不是 Electron 宿主。

## 必须保留

- Rust UDP / HID 接收、算法、校准、AutoBone、SteamVR、OSC / VMC、SolarXR 和 BVH 功能。
- SolarXR 与 OpenVR 子模块、OpenVR helper、驱动下载和平台资源构建脚本。
- 两套现用桌面界面和 Overlay、引导、翻译、主题、模型、字体、音效及实际使用的素材。
- 原版 YAML 读写、GUI 偏好兼容、设备固件更新与 Discord Rich Presence。旧偏好中的未知字段保留，不强制删除用户本地设置。
- 算法 golden fixtures 和参考生成工具；仅重新生成上游参考数据时需要 Java / Kotlin。
- 原始许可证、版权与商标说明、移植数学代码及分发资源的第三方许可。

## 当前开发入口

后端：`cargo build --manifest-path server-rust/Cargo.toml --release --locked`。

GPUI：参见 [原生界面说明](../gui-gpui/README.zh-CN.md)。Tauri：使用 `pnpm gui` 或 `pnpm tauri:rust:build`，参见 [Tauri 指南](../gui/README.tauri.md)。浏览器开发仍使用 `pnpm web`，另外运行后端。

CI 使用统一 AIO 工作流，保留网页检查和三套 Rust 分发包，见 [Actions 说明](rust-ci.zh-CN.md)。历史设计文档中的 Electron / Java 描述是当时的迁移背景，当前入口以根目录 README 和各组件指南为准。删除的上游源码可从 Git 历史查询，不复制到现用工作区。
