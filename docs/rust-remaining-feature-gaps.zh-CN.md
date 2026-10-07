# Rust 与原版的功能状态及验收边界

核对日期：2026-10-04。参考提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

此前清单中的正常流程和外围功能均已接入。本文件区分已完成实现与仍需真实环境验证的项目；不宣称任意硬件、配置和时序下与 Java 完全等价。详细变更及构建方式见 [实施交接](rust-completion-worklog.zh-CN.md)。

## 功能状态

| 功能 | 当前实现 | 尚需实机验收 |
| --- | --- | --- |
| UDP、部位分配、YAML | 原协议、会话 / 乱序处理、原配置版本迁移、原子保存、未知字段保留 | 原有设备、多 sensor、断网重连 |
| 核心算法与校准 | FK、扩展模型、约束、滤波、腿部修正、StayAligned、Localizer、Full / Yaw / Mounting / Feet、局部复位、清除安装、tap / flex / 身高校准 | 真实动作、佩戴、热机与漂移 |
| AutoBone、BVH | 逐轮训练、缺少部位、保存 / PFS-PFR 导入 / 重启加载、应用；原 BVH 层级与采样 | 真实录制训练与导出软件加载 |
| 磁力计与检查清单 | ACK / 超时、全局 / 单 sensor 偏好；网络、设备、平台和校准检查 | 固件版本差异、系统网络类别 |
| 串口与固件 | 枚举、配网、控制、OTA、ESP32 / ESP8266 串口刷写、下载校验和取消 | 实际 USB 芯片、Boot 引脚和固件 |
| USB HID | 16 字节报告、遥测、按键、休眠、拔插、身体绑定 | 实际接收器与速率 |
| SteamVR | 驱动注册 / 启用 / 安全模式、协议 2、协议 1 feeder、Vive / Tundra、共享、HMD / 控制器、原版 Bindings Provider | Windows 管道、OpenVR 按钮与真实追踪 |
| OSC / OSCQuery / VMC | 端口重绑、路由、发现、VRChat、VMC / VRM、镜像 / 手指、原 YAML / RPC 设置 | VRChat / Unity / VMC 对端与局域网发现 |
| VRChat 与 overlay | 偏好读写、推荐值 / 忽略项、overlay 设置、跨 WebSocket / 原生 IPC pub/sub | 当前客户端版本、实际 overlay |
| 快捷键与 Tauri | Windows 全局快捷键、Discord Presence、托盘 / 对话框 / 子进程 / 退出、手动版本提示与宿主打包 | Windows / macOS 原生安装和系统服务 |
| 旧式 WebSocket | config / pos / action，与 SolarXR 同端口；原 11 点及 HMD 高度偏移 | 真实旧客户端 |

`usePosition` / `correctConstraints` 兼容读写按原版普通 FK 路径处理：原版通用 IK 的开关不影响正常 FK 更新，约束反馈在普通路径硬编码关闭。`extraYawCorrection` 与 reset-history drift compensation 在参考版本已废弃 / 禁用；保留这些 YAML 字段不表示重新启用算法。UDP 包 200 的协议协商在原版也没有执行切换。

## 仍存在的验证和行为边界

- 核心参考为 95 组，含 computed 来源复位、flex 与 YXZ 奇异角。tap 和身高校准有状态测试，尚无完整 Kotlin 状态机差分。
- AutoBone 完整训练对照为 14 组，新增缺少单 / 双小腿、单腿、仅躯干、仅 HMD。离散结果严格一致；一般骨长容差 0.1 mm，仅 HMD 用例 1 mm；不覆盖所有录制组合。
- Rust 默认 4 ms tick，不逐次复现 Java 实时调度。派生速度从最终后处理位置和毫秒时钟计算，原版在 FK 阶段用微秒时钟求导，见 [说明](rust-derived-velocity.zh-CN.md)。
- 无世界锚点且地面未初始化时，先等待 Full reset 或 Localizer 再修正脚部。
- 配置保持原字段和未知字段，但 YAML 注释 / 排版不保留；保存基于加载的树，不合并另一个进程的后续改动。GUI 自身偏好仍由桌面适配器保存。
- 消息、VRM、录制导入、下载和队列有明确上限；无效输入返回错误。不是无限制文件兼容。VRM JSON 最大 4 MiB，SolarXR WebSocket 最大 8 MiB。
- macOS 不提供 SteamVR 桌面桥；系统全局快捷键沿用原版 Windows 范围。更新沿用手动 GitHub release 提示，不提供签名后台自动安装；只有配置了 `VITE_RELEASE_REPOSITORY` 才检查对应发布仓库。
- Linux Debian 包已构建；Windows GNU 后端交叉编译已通过。Windows / macOS 原生运行、真实硬件与长时间性能尚未验证，四平台 CI 工作流已准备但未执行。

## 统一测试

见 [实机清单](rust-unified-hardware-test.zh-CN.md)。出现问题时记录平台、版本、设备固件、配置副本、日志、操作步骤及是否能在 Java 参考版本复现。协议和解算问题可用原始 journal 重放；不要同时启动两个后端争用同一端口或配置。
