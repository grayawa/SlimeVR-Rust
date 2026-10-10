# Changelog

本文件记录 SlimeVR-Rust 独立开发预览的变更。当前使用与接口契约见 [文档目录](docs/README.md)，构建包见 [分发指南](docs/rust-distribution.zh-CN.md)。历史条目按已知日期和提交归类。

## Unreleased

- 整理文档目录，将 34 份说明合并为 18 份专题指南并增加索引；桌面包使用统一指南与变更记录。

## 2026-10-10

- Linux GPUI 提供 StatusNotifier 托盘、语言同步、最小化和退出；完整 tar.gz 包包含后端、驱动、OpenVR helper、USB / HID 权限规则与许可通知。[PR #19](https://github.com/grayawa/SlimeVR-Rust/pull/19)
- Linux 构建检查 ELF 架构、运行库、执行权限与文件哈希，并执行 D-Bus 托盘、真实后端回环及 X11 软件 Vulkan 渲染检查。Wayland 和真实 SteamVR / VRChat 按实机清单验收。
- 将设置组合组件提取为独立的 `slimevr-ui` crate，桌面、Overlay 与预览共用主题角色和组件接口。[PR #18](https://github.com/grayawa/SlimeVR-Rust/pull/18)
- README 开头加入 AI 使用、Code review、产物测试和上游项目链接声明。[PR #20](https://github.com/grayawa/SlimeVR-Rust/pull/20)

## 2026-10-09

- UDP 分别记录 socket 接收时刻和主循环处理时刻，增加姿态年龄、排队和批处理诊断；接收器复用解析结果，UDP 批处理按包数与时间预算调度。[PR #12](https://github.com/grayawa/SlimeVR-Rust/pull/12)
- 配置 revision 与 dirty 标志驱动导出、热键和 OSC 重配。[PR #12](https://github.com/grayawa/SlimeVR-Rust/pull/12)
- API 使用共享配置、共享姿态与设备元数据缓存；配置写入交给独立线程，客户端保存确认等待持久化 revision。[PR #13](https://github.com/grayawa/SlimeVR-Rust/pull/13)
- 增加 `Service::live()`、配置保存阶段、SteamVR 输出队列和实际 IPC 批次诊断。快照与负载基准的测量条件和原始数据见 [性能验证](docs/rust-udp-runtime-performance.zh-CN.md)。
- 项目新增贡献采用 GPL-3.0-or-later，保留继承代码和资源的许可证；发行身份为 SlimeVR-Rust 独立开发预览。[PR #14](https://github.com/grayawa/SlimeVR-Rust/pull/14)、[PR #16](https://github.com/grayawa/SlimeVR-Rust/pull/16)
- Tauri 安装资源与下载产物携带许可、第三方通知和对应源码信息；SteamVR 仪表盘入口使用独立开发预览标识。[PR #16](https://github.com/grayawa/SlimeVR-Rust/pull/16)、[PR #17](https://github.com/grayawa/SlimeVR-Rust/pull/17)

## 2026-10-08

- 生产源码与构建入口采用 Rust 后端、GPUI 和 React / Tauri，整理 Java / Gradle、Electron 与上游集成文件。[PR #6](https://github.com/grayawa/SlimeVR-Rust/pull/6)、[PR #7](https://github.com/grayawa/SlimeVR-Rust/pull/7)
- GPUI 通信线程将重置进度直接送入音频线程，启动时预解码音效，按会话、事务、阶段和秒数去重。[PR #8](https://github.com/grayawa/SlimeVR-Rust/pull/8)
- 后端独立接收 UDP、写诊断日志，增加 tick jitter / work 的 p50、p95、p99、p999、max 和多阈值 runtime stall 统计。[PR #9](https://github.com/grayawa/SlimeVR-Rust/pull/9)
- UDP 积压时按来源、sensor、字段覆盖与序号边界合并旧纯姿态包，控制消息保持顺序。[PR #10](https://github.com/grayawa/SlimeVR-Rust/pull/10)
- GPUI、Tauri、Overlay 提供独立构建工作流，AIO 用于发布合集。[PR #11](https://github.com/grayawa/SlimeVR-Rust/pull/11)

## 2026-10-07

- 提交 Rust 后端、GPUI 原生界面和 Tauri 宿主，包含 YAML、算法参考、AutoBone、SteamVR、OSC / VMC、串口 / HID 和 BVH。[b0618833](https://github.com/grayawa/SlimeVR-Rust/commit/b0618833)
- 提供 SteamVR 仪表盘附加包；隐藏宿主初始化客户区尺寸后提交纹理，面板支持调宽、桌面列表偏好同步和骨架缩放。[7909acf0](https://github.com/grayawa/SlimeVR-Rust/commit/7909acf0)、[e776db48](https://github.com/grayawa/SlimeVR-Rust/commit/e776db48)、[2a57241d](https://github.com/grayawa/SlimeVR-Rust/commit/2a57241d)
- 完善 Windows 单实例唤起与显式 BVH 保存路径通知。[3906fffe](https://github.com/grayawa/SlimeVR-Rust/commit/3906fffe)、[38988d76](https://github.com/grayawa/SlimeVR-Rust/commit/38988d76)

### 初始实现中的修复归档

以下条目原先分散在 `windows-fix1–fix5`、手部切换和重连说明中；这组修复随初始代码提交收录。当前行为分别归入专题指南。

- WebSocket 监听器按连接和组件生命周期管理，异常记录包含断开码、原因和状态；分配页按页面 / 连接变化切换 `setupMode`。[联调指南](docs/rust-frontend-integration.zh-CN.md#连接与分配页面生命周期)
- 发现广播使用 10 秒安静窗口，主动握手进入正常准入流程。[设备操作](docs/rust-device-operations.zh-CN.md#发现与连接)
- 重置进度按整秒通知，前端处理重复进度并播放对应素材；左右腿佩戴与分配在实机清单中核对。[实机清单](docs/rust-unified-hardware-test.zh-CN.md)
- 已有 SteamVR 驱动安装和注册保留，状态提示使用 Fluent；手部追踪 / 手柄切换按有效姿态来源所有者处理节点清理。[SteamVR 指南](docs/rust-steamvr-bridge.zh-CN.md)
- 同一 UDP 身份重新握手时保留旋转校准，新会话重建动态历史。[会话契约](docs/rust-backend-architecture.zh-CN.md#udp-会话与校准)
- 前端和桌面启动的后端使用统一日志级别，桌面日志轮转保留当前文件与四份历史文件。[日志指南](docs/rust-logging.zh-CN.md)

## 公开准备核对记录（2026-10-10）

核对记录的源码基线为 `9aee2878`；安装许可与发行身份补充检查使用 `b22ba48a`。这些记录描述当次扫描和抽查范围，后续发布按 [发布清单](docs/release-checklist.zh-CN.md) 重新核对。

- Cargo metadata 覆盖后端 317、GPUI 950、Tauri 514 个多平台及开发依赖包；检查声明中的 GPLv3 兼容授权路径，并保留 MPL、字体、图像和第三方通知义务。实际链接集合取决于平台与 feature。
- Gitleaks 8.30.1 扫描本地可达历史的 2,496 个提交、约 62.90 MB 文本；两处候选项对应上游 Java 的 `Advapi32.INSTANCE`，人工核对为误报。
- 可访问的 15 个 PR 说明与 114 次 Actions 日志扫描规则匹配数为零；当次 API 返回的 Issue 评论和 PR 行内评论数量为零。图片和外部附件属于人工核对范围。
- 跟踪文件检查覆盖日志、vrconfig、证书私钥和上传归档，这些类别的匹配结果为零。跟踪的 `gui/.env` 包含三个固件服务 URL 配置变量。
- 抽查 Tauri 解压包的 70 个文件条目与文本；随后检查 `b22ba48a` 的 Tauri / GPUI / Overlay 归档分别 87 / 88 / 40 个条目，私人配置、日志、私钥文件名候选项与密钥规则匹配数为零。Tauri 的七份项目许可 / 通知与源码一致，源码引用和 manifest 对应构建提交。
- `b22ba48a` 的日常 CI 与三个独立 Windows 应用构建通过；Windows 安装、音频、真实追踪及其余 artifacts / 附件按各自实机或公开资料检查记录确认。
