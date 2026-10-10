# 功能状态与验收范围

后端与两套桌面界面实现现有 SlimeVR 的常用接收、算法、设备管理和输出流程。使用与开发入口见 [文档目录](README.md)，参考版本为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

## 功能与实机项目

| 功能                  | 当前实现                                                                                                 | 实机验收内容                           |
| --------------------- | -------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| UDP / HID、分配与配置 | 多设备 / 多 sensor、序号 / 会话、准入、YAML 迁移与后台原子保存                                           | 固件组合、断网、拔插、多 sensor 与速率 |
| 算法与校准            | FK、扩展模型、约束、滤波、腿部修正、StayAligned、Localizer、Full / Yaw / Mounting、tap / flex / 身高校准 | 真实动作、佩戴、热机和漂移             |
| AutoBone / BVH        | 训练、保存、PFS / PFR、应用、原版 BVH 层级和采样                                                         | 真实录制拟合与 Blender 等工具导入      |
| 串口与固件            | 配网、控制、OTA、ESP32 / ESP8266 刷写、校验和取消                                                        | USB 芯片、Boot 引脚、板型和失败恢复    |
| SteamVR               | 驱动管理、协议 2、协议 1 feeder、HMD / 控制器、Vive / Tundra、共享和 Bindings Provider                   | Windows 管道、按钮、手部切换和真实追踪 |
| OSC / OSCQuery / VMC  | 路由、端口、发现、VRChat、VMC / VRM、镜像和手指                                                          | 对端版本、局域网发现与坐标             |
| 桌面                  | GPUI / Tauri、引导、Fluent、预览、草稿、快捷键、托盘、日志和 Discord                                     | 输入法、DPI、平台路径、系统服务和音频  |
| 仪表盘                | 重置、骨架、节点信息、宽度与桌面偏好同步                                                                 | 纹理、手柄命中、GPU、清晰度与宽度      |
| 旧 JSON WebSocket     | `config` / `pos` / `action`，共用 SolarXR 端口                                                           | 实际旧客户端                           |

## 当前契约与测试边界

- 核心 fixture 含 99 组参考，AutoBone 完整训练含 14 组。tap 与身高校准主要使用状态测试，flex 使用 Kotlin 差分。具体输入与容差见 [核心验证](rust-core-validation.zh-CN.md)。
- Rust 默认 tick 为 4ms，实际调度由操作系统决定。派生速度使用最终后处理位置和毫秒时钟，参考 Java 路径在 FK 阶段使用微秒时钟。见 [速度说明](rust-steamvr-bridge.zh-CN.md#派生速度)。
- 世界坐标输出要求有效锚点；地面初始化后执行脚部修正。单 sensor 停更沿用缓存姿态可用性规则，年龄指标用于诊断。
- `usePosition` / `correctConstraints` 按普通 FK 的兼容语义保存；约束反馈和 reset-history drift compensation 保持参考路径的关闭状态。`extraYawCorrection` 接受后忽略，协议包 200 按参考规则忽略。
- YAML 保存保留未映射字段，序列化器生成排版。运行期以加载并更新的内存树为配置来源，同一配置文件由一个后端管理。
- 输入和队列按大小校验：VRM JSON 最大 4MiB，SolarXR WebSocket 最大 8MiB。AutoBone 导入、固件下载与控制队列使用各自的容量限制。
- SteamVR 桌面桥支持 Windows / Linux；仪表盘纹理提交使用 Windows D3D11，Linux Overlay 可用于桌面预览。GPUI 的 Linux 包以 Ubuntu 24.04 为基线，自动窗口检查覆盖 X11 软件 Vulkan。全局快捷键支持 Windows。Tauri 的 Linux / macOS 构建使用独立平台选项，具体范围见 [CI](rust-distribution.zh-CN.md)。
- 软件包通过本仓库 Actions artifacts 获取，设备固件通过固件服务更新。性能结论来自相同场景的实测数据，见 [基准](rust-udp-runtime-performance.zh-CN.md)。

## 源码入口

| 范围                 | 入口                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------ |
| 接收与调度           | `server-rust/crates/slimevr-server/src/receiver.rs`、`receiver/`、`runtime/`               |
| 算法与校准           | `server-rust/crates/slimevr-core/src/`                                                     |
| 通信、配置与设备操作 | 服务端 `api/`、`config.rs`、`config/`、`serial/`、`firmware.rs`、`hid.rs`                  |
| SteamVR 与 OSC / VMC | 服务端 `steamvr/`、`osc/`；`bindings-provider/`                                            |
| 录制与导出           | 服务端 `recording.rs`、`pose_recording.rs`、`bvh.rs`                                       |
| 桌面、组件与仪表盘   | `gui-gpui/src/`、`gui-gpui/ui/`、`gui-gpui/overlay-runtime/`、`gui/src/`、`gui/src-tauri/` |

表中服务端路径以 `server-rust/crates/slimevr-server/src/` 为根。

## 验收记录

按 [实机清单](rust-unified-hardware-test.zh-CN.md) 记录操作系统、代码版本、设备固件、配置、操作与日志时间。给出每项“通过 / 失败 / 未测”及复现条件。协议和状态问题可用原始 journal 重放，平台与真实追踪结果按对应设备另行记录。
