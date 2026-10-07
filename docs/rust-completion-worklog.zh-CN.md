# Rust / Tauri 实施交接

日期：2026-10-04。原版参考提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

此前列出的功能缺口已完成实现，可进入集中实机测试。日常后端配置继续使用原版 `vrconfig.yml` / `vrconfig.yaml`；不增加新的 JSON 配置。当前结论依据源码对照、可控输入、模拟设备和本地构建，不代表所有真实硬件和平台均已验收。

2026-10-07 更新：`api/mod.rs` 已拆为配置、连接 Session、应用状态与领域 RPC 模块，公共入口保持兼容；当前导航与验证见 [后端 API 架构说明](rust-backend-api-architecture.zh-CN.md)。

## 本轮完成

| 范围 | 具体实现 | 主要入口 |
| --- | --- | --- |
| 串口与配网 | 枚举、热插拔、端口占用、日志订阅、凭据遮蔽、Wi-Fi 写入、重启、恢复出厂、获取 MAC / Wi-Fi 扫描、原版配网状态和超时；成功设备写入 knownDevices | `server-rust/crates/slimevr-server/src/serial.rs`、`serial/` |
| 固件更新 | 有界流式下载与 MD5 / SHA-256 / SHA-512 校验；原版 ESP OTA 认证、分块 ACK、重连完成；ESP32 espflash、ESP8266 ROM 串口刷写；取消等待工作进程退出后归还串口 | `firmware.rs`、`serial/esp8266.rs` |
| USB HID | 原版 16 字节报告、设备注册、会话隔离、两种旋转编码、惯性和电池遥测、按键、休眠、拔插；原 YAML 身体绑定、journal v4 回放 | `hid.rs`、`receiver/hid.rs` |
| SteamVR 来源与分发 | 协议 2 输出桥、协议 1 SlimeVRInput feeder、Vive / Tundra 身体输入、默认角色与共享、失效 / 恢复；原版 OpenVR Bindings Provider 随包；驱动注册、启用和安全模式管理 | `steamvr/`、`gui/scripts/build-bindings-provider.py` |
| 外部定位源 | 来源能力标记、原 YAML 绑定和名称、重连保留配置、会话过期拒绝、失去位置时清理缓存、非 IMU 的复位和滤波语义 | `receiver/external.rs`、`slimevr-core/src/pose.rs` |
| OSC / OSCQuery / VMC | 可重绑 UDP 输入输出与路由、mDNS 发现和 HTTP 查询、VRChat 坐标与 yaw reset、VMC root / tracker / bone、原版 UnityArmature、VRM0 / 1、镜像和手指 | `osc/` |
| VRChat 与 overlay | Windows / Steam-Wine 偏好读取与推荐值写入、检查清单忽略项、可定位控制器检测、overlay 显示设置；WebSocket 和原生 IPC 共用 pub/sub，支持连接间通信 | `api/vrchat.rs`、`api/pubsub.rs` |
| 桌面 | Windows 系统快捷键与延迟动作、Tauri Discord Presence、原版手动 GitHub 版本提示、宿主构建脚本和四平台手动 CI | `hotkeys.rs`、`gui/src-tauri/src/presence.rs`、`.github/workflows/rust-desktop.yml` |
| 校准与算法边界 | 清除安装校准、手动方向变更保留运行校准、Full 后安装窗口及完成状态、替换已占用身体绑定；computed 来源复位、flex、YXZ 奇异角、缺少身体部位的 AutoBone | `slimevr-core/`、`api/service/calibration.rs`、`api/service/rpc/`、`tools/` |
| 旧式 WebSocket | 原版 config / pos / action；11 个输出点顺序、HMD 的 0.2 m 偏移、五种校准 / 暂停动作；同端口兼容 SolarXR | `api/service/legacy.rs`、`api/transport/` |

此前的 UDP、FK、LegTweaks、StayAligned、Localizer、敲击分配、磁力计命令、AutoBone 文件保存 / PFS-PFR 加载、BVH 和 YAML 迁移保留。串口、OSC、HID 等后台任务有独立所有权和退出路径；前端 false / 0 和空 pub/sub payload 编码也已修正。原版注册的 53 种 RPC 请求均已核对，包括废弃的高度查询和漂移清除兼容入口。

## 自动验证

- Rust 工作区：**112 项测试**通过；Clippy `-D warnings` 通过。
- Kotlin 核心参考：**95 组**；AutoBone 完整训练参考：**14 组**；普通测试读取 fixture，不要求运行 Java。
- 前端 / 桌面 Node：**17 项测试**，含真实 Rust 进程、UDP、WebSocket、原生 IPC、OSC、4 MiB VRM 和旧式 JSON 接口。
- Tauri Rust：**5 项测试**，含双向 Discord 模拟会话；最终 Linux 桌面烟雾测试验证启动 Rust 后端、SolarXR 设置修改落盘、窗口正常关闭和后端回收。
- TypeScript、修改入口的 ESLint、Vite 生产构建、Linux release、Windows GNU 后端交叉编译和 Linux Tauri Debian 打包通过。

这些测试覆盖数据与状态转换、文件格式、请求 / 响应和进程生命周期；不代替真实串口刷写、无线环境、SteamVR 追踪品质、Windows 系统快捷键及原生安装体验。

## 启动和构建

从仓库根目录运行，需已安装 Node / pnpm、Python 3、Rust stable，以及平台的 Tauri / CMake / C++ 构建依赖：

```sh
pnpm install
pnpm tauri:rust:dev
# 构建本机安装包
pnpm tauri:rust:build
```

命令更新 SolarXR 绑定、构建 Rust 后端、获取固定版本官方驱动并构建原版 Bindings Provider，然后启动或打包 Tauri。首次驱动下载和源码依赖拉取需要网络。macOS 沿用原版平台范围，不提供 SteamVR 桌面桥。

当前 Linux 产物：`gui/src-tauri/target/release/bundle/deb/SlimeVR_0.1.0_amd64.deb`。Windows / macOS 原生包应在对应系统运行构建，工作流仅 `workflow_dispatch` 触发、上传构建产物，不发布 release。尚未在远程 CI 执行该工作流。

也可以独立启动后端再连接网页，见 [联调启动说明](rust-frontend-integration.zh-CN.md)；保留原 `--config` / `--no-steamvr` 等参数。开发环境如需减少构建缓存，可设置 `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`。

## 交给实机验收的内容

统一按 [实机测试清单](rust-unified-hardware-test.zh-CN.md) 执行。优先跑现有六点 Wi-Fi / SteamVR，再测试新增来源与外围操作。

尚未实测：真实追踪器、真实 OTA / ROM 刷写、SteamVR / OpenVR 输入、Windows 原生命名管道 / 快捷键 / 注册表、macOS 安装，以及长时间无线与追踪性能。Linux 构建和 Windows 交叉编译不能证明这些项目已通过。算法仍有明确的浮点 / 时序和参考覆盖边界，见 [剩余差异](rust-remaining-feature-gaps.zh-CN.md) 与 [核心验证](rust-core-validation.zh-CN.md)。

Windows 11 x64 便携包已交叉构建并检查依赖 / 完整性；按用户要求不附 WebView2 Runtime，见 [便携包说明](rust-windows-portable.zh-CN.md)。Windows 原生实测状态仍为待验收。
