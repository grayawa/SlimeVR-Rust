# 项目功能与验收入口

项目包含 Rust 后端、GPUI 原生桌面、React / Tauri 桌面和 SteamVR 仪表盘。上游参考版本为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。后端直接使用原版 `vrconfig.yml` / `.yaml`，GUI 偏好由前端保存。

## 功能与源码

| 范围       | 当前功能                                                                                         | 入口                                                                                                     |
| ---------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| 接收       | UDP 多设备 / 多 sensor、HID、准入、握手、遥测、心跳、序号、超时与重连                            | `server-rust/crates/slimevr-server/src/receiver.rs`、`receiver/`、`runtime/ingress.rs`                   |
| 算法       | 全身 FK、约束、滤波、LegTweaks、StayAligned、Localizer、flex、tap 与派生速度                     | `server-rust/crates/slimevr-core/src/`                                                                   |
| 校准与比例 | Full / Yaw / Mounting、HMD pitch、脚 / 手指、身高校准、AutoBone、PFS / PFR 与比例导入导出        | `slimevr-core/src/calibration.rs`、`autobone.rs`；服务端 `api/service/recording.rs`、`pose_recording.rs` |
| 前端通信   | SolarXR、旧 JSON WebSocket、原生 RPC、data feed、pub/sub 与检查清单                              | 服务端 `api/`                                                                                            |
| 配置       | YAML 版本迁移、未知字段保留、共享配置、后台顺序保存与持久化确认                                  | 服务端 `config.rs`、`config/`、`api/shared_config.rs`                                                    |
| SteamVR    | 协议 2 输出、协议 1 输入 feeder、HMD / 控制器和 Vive / Tundra、共享、驱动管理、Bindings Provider | 服务端 `steamvr/`、`bindings-provider/`                                                                  |
| OSC / VMC  | 可重绑端口与路由、OSCQuery / mDNS、VRChat、VMC root / tracker / bone、VRM0 / 1、镜像与手指       | 服务端 `osc/`                                                                                            |
| 配网与固件 | 串口枚举、日志、Wi-Fi、命令、OTA、ESP32 / ESP8266 刷写、校验与取消                               | 服务端 `serial/`、`firmware.rs`                                                                          |
| 录制       | 原始事件 journal、场景回放、AutoBone 文件、BVH 流式导出                                          | 服务端 `recording.rs`、`pose_recording.rs`、`bvh.rs`                                                     |
| 桌面       | 引导、原版侧栏与设置、三维预览、Fluent、偏好、托盘、日志、音效和 Discord Presence                | `gui-gpui/`、`gui/src`、`gui/src-tauri/`                                                                 |
| 仪表盘     | 重置、骨架、节点列表、尺寸与桌面偏好同步                                                         | `gui-gpui/src/bin/overlay.rs`、`overlay-runtime/`                                                        |

表中的 `slimevr-core/` 和服务端相对路径分别以 `server-rust/crates/slimevr-core/` 和 `server-rust/crates/slimevr-server/src/` 为根。

## 开发与构建

后端、算法和页面开发从 [根 README](../README.md) 的入口开始。通信联调见 [前后端联调](rust-frontend-integration.zh-CN.md)，线程、快照与保存边界见 [API 架构](rust-backend-api-architecture.zh-CN.md)。

PR 使用 **SlimeVR Checks**。Windows 测试包通过独立的 **Build GPUI**、**Build Tauri**、**Build Overlay** 获取，AIO 用于发布合集。平台选项、产物和下载方式见 [CI](rust-ci.zh-CN.md)。

## 自动验证与实机验收

自动验证覆盖协议字节、受控输入的算法差分、状态机、回放、配置、通信和打包。核心与 AutoBone 参考、容差和重建方法见 [核心验证](rust-core-validation.zh-CN.md)。性能统计、微基准和负载测量见 [runtime 性能](rust-udp-runtime-performance.zh-CN.md)。每次提交的检查结果以对应 Actions 运行记录为准。

实机验收优先采用六点 Wi-Fi / SteamVR 流程，再按硬件验证 OTA、ROM 刷写、HID、OSC / VMC、系统快捷键和平台安装。记录代码版本、配置、固件、步骤与日志时间。各功能的覆盖范围见 [功能状态](rust-remaining-feature-gaps.zh-CN.md)，执行步骤见 [统一清单](rust-unified-hardware-test.zh-CN.md)。
