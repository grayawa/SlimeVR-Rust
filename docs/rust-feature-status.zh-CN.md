# 平台支持与验收边界

项目处于独立开发预览阶段。功能入口见 [项目 README](../README.md)，源码结构见 [后端架构](rust-backend-architecture.zh-CN.md) 和 [GPUI 指南](rust-gpui-guide.zh-CN.md)。参考版本为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

## 平台范围

| 平台        | 桌面与集成范围                                                                                              | 验收范围                                                                                       |
| ----------- | ----------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Windows x64 | GPUI / Tauri 完整包、SteamVR 桥、D3D11 仪表盘和全局快捷键                                                   | CI 覆盖构建、状态与通信；真实追踪、输入、显卡和音频按实机清单记录                              |
| Linux       | GPUI x64 包以 Ubuntu 24.04 为基线，支持 Vulkan / StatusNotifier；SteamVR 桥支持 Linux，Overlay 提供桌面预览 | 自动窗口检查覆盖 X11 软件 Vulkan 和 D-Bus 托盘；Wayland、无线设备、SteamVR / VRChat 按实机记录 |
| macOS       | Tauri 提供平台构建选项；可使用浏览器、OSC 和 VMC                                                            | 具体构建结果与原生交互按对应版本记录                                                           |

Tauri Linux / macOS 包通过独立平台构建选项获取。固件组合、双 sensor、HID、串口、OTA、板型、对端协议与无线表现按实际硬件记录。下载与检查入口见 [分发指南](rust-distribution.zh-CN.md)。

## 当前契约与测试边界

- 核心 fixture 含 99 组参考，AutoBone 完整训练含 14 组。tap 与身高校准主要使用状态测试，flex 使用 Kotlin 差分。具体输入与容差见 [核心验证](rust-core-validation.zh-CN.md)。
- Rust 默认 tick 为 4ms，实际调度由操作系统决定。派生速度使用最终后处理位置和毫秒时钟，参考 Java 路径在 FK 阶段使用微秒时钟。见 [速度说明](rust-steamvr-bridge.zh-CN.md#派生速度)。
- 世界坐标输出要求有效锚点；地面初始化后执行脚部修正。单 sensor 停更沿用缓存姿态可用性规则，年龄指标用于诊断。
- `usePosition` / `correctConstraints` 按普通 FK 的兼容语义保存；约束反馈和 reset-history drift compensation 保持参考路径的关闭状态。`extraYawCorrection` 接受后忽略，协议包 200 按参考规则忽略。
- YAML 保存保留未映射字段，序列化器生成排版。运行期以加载并更新的内存树为配置来源，同一配置文件由一个后端管理。
- 输入和队列按大小校验：VRM JSON 最大 4MiB，SolarXR WebSocket 最大 8MiB。AutoBone 导入、固件下载与控制队列使用各自的容量限制。
- 性能结论限定到测量环境、负载和样本，实际调度与输出延迟按目标机器测量，见 [性能基准](rust-udp-runtime-performance.zh-CN.md)。

## 验收记录

按 [实机清单](rust-unified-hardware-test.zh-CN.md) 记录操作系统、代码版本、设备固件、配置、操作与日志时间。每项标记“通过 / 失败 / 未测”及复现条件。协议和状态问题可用 journal 重放，真实追踪与平台行为按对应设备记录。
