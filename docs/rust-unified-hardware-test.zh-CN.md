# Rust / GPUI / Tauri 统一实机测试清单

功能实现与自动验证见 [交接记录](rust-completion-worklog.zh-CN.md)。按现有六点流程优先测试，再按手头硬件选择外围项目；没有对应硬件的项标记“未测”。

## 准备

1. 复制现有 `vrconfig.yml` / `vrconfig.yaml` 和 AutoBone 录制目录，用副本测试；确保接收端口和测试配置由目标后端独占。
2. 运行 `pnpm tauri:rust:dev`，或安装本机构建产物。确认界面识别 Rust 后端、设置 / 身体绑定读取正常，日志无持续报错。
3. 修改一个设置、关闭并重启，确认恢复；检查设置写入 YAML、原有未知字段保留且 `.bak` 可用。若往返 Java，先关闭 Rust。

## 日常六点与算法

- [ ] 六台 Wi-Fi tracker 连接、批准、命名和分配；sensor 0 / 多 sensor 正常；重复分配替换旧部位。
- [ ] 断网、设备重启、重新加入、删除后批准；当前会话姿态生效、失效来源位置清理。
- [ ] Full → Yaw → Mounting；默认 / 开启脚部安装；HMD 俯仰开关、局部复位、清除安装；手动安装方向保存、重启恢复。
- [ ] 暂停 / 恢复、tap 校准和两次敲击分配；未分配 tracker 可被识别；Windows 快捷键含延迟、SteamVR 按钮绑定。
- [ ] 站立、转身、行走、蹲、坐、躺；腿部开关、StayAligned / Localizer；HMD / 控制器锚点丢失后恢复。
- [ ] 重复原来的 CPU 密集任务，观察追踪器在线时点位是否错乱；负载下降、设备重新握手后应保留校准并恢复。记下时间并核对 `runtime_timing`（jitter 分位数与 stall 计数）/ `device_connected`，详见 [重连校准修复](rust-load-reconnect.zh-CN.md)。
- [ ] 自动身高校准成功、超时 / 取消；骨长应用保存。AutoBone 录制 / SAVE / PROCESS / APPLY、重启加载、PFS-PFR 导入、应用要求训练成功并通过接受阈值。
- [ ] 磁力计全局 / 单 tracker 开关，实际 ACK、超时 / 重连与状态反馈。
- [ ] BVH 开始 / 停止、改变骨架、正常退出；在 Blender 等工具导入，层级 / 动作 / 帧率合理。

## SteamVR

- [ ] 首次驱动注册；已有 / 手动安装保护；启用、解除安全模式、重启请求与失败提示。
- [ ] 共享角色、11 点输出、头显 / 控制器输入、按钮复位；SteamVR 断开 / 重连时辅助进程按生命周期退出和重建。
- [ ] Vive / Tundra 身体 tracker 分配、命名、拔插恢复；自动共享按来源与角色规则执行。
- [ ] 需要兼容旧驱动时单独测试 `SlimeVRInput` 协议 1；协议 2 连接时使用主输入通道。
- [ ] 原版 Bindings Provider 随包启动、OpenVR action manifest / 按钮正确。

## 串口、固件与 HID

- [ ] USB 枚举 / 热插拔、正确端口选择、日志订阅、关闭回收；Wi-Fi 配网成功 / 错误密码 / 超时，knownDevices 更新；日志中的凭据遮蔽生效。
- [ ] 重启、恢复出厂、Wi-Fi 扫描；其他程序占用端口时有可理解的错误。
- [ ] 选择适配设备的固件，先测试下载校验失败和取消，再测试 OTA / ESP32 / ESP8266 刷写；观察进度、重启后完成和失败恢复。取消后端口可重新打开。
- [ ] HID 开关、实际接收器、旋转 / 加速度 / 电量、按键分配、休眠、拔插；同名设备绑定在重启后保留。
- [ ] Linux 权限不足时检查 udev 提示并使用随包规则。

## OSC、VRChat、VMC 与桌面

- [ ] OSC 输入 / 输出、路由转发、端口共享 / 改端口 / 禁用；关闭后端口释放。
- [ ] OSCQuery 能被发现；VRChat tracker / HMD / 控制器姿态、坐标和 yaw reset；mDNS 选中真实 VRChat 对端。
- [ ] VMC tracker / bone / root 输入输出；VRM0 / 1、模型高度、镜像、左右手 / 手指；来源位置失效后清理旧锚点。
- [ ] VRChat 偏好推荐值、写入和重新读取，检查清单忽略项、控制器检测；实际 overlay 可见 / 镜像与跨 IPC pub/sub。
- [ ] Discord Presence 开启 / 更新 / 关闭及 Discord 迟启动 / 重启；托盘、保存对话框、日志、正常退出和异常重启，Rust 后端 / 刷写子进程均被回收。
- [ ] 旧式 JSON WebSocket 客户端：11 个 config / pos，HMD 输入与校准 / 暂停动作。
- [ ] Windows / Linux / macOS 对应安装包启动；按平台支持范围测试。软件包从本仓库 Actions artifacts 获取。
- [ ] Linux GPUI：X11 / Wayland 分别启动；StatusNotifier 托盘显示、最小化、恢复、切换语言和退出；启用托盘后关闭窗口保持后端运行。托盘服务退出后，窗口关闭走正常退出流程。
- [ ] Linux 解压目录含空格时启动，配置和日志写入 XDG 目录；驱动注册后，头显 / 控制器输入与追踪器输出在 SteamVR / VRChat 中验收。

## 持续运行与问题记录

至少运行 30 分钟，观察 CPU、内存、实际延迟、无线丢包、温度和漂移；切换场景、睡眠恢复和退出重启。性能结论采用该次实机测量的环境、配置与记录。

记录：操作系统、代码版本、设备型号 / 固件、配置副本、复现步骤、期望 / 实际、日志时间、是否能在原 Java 服务复现。需要 journal 时独立启动后端并用 `--record <新文件.jsonl>`；BVH 用于动画导出，journal 用于原始协议与事件回放。日志和配置中的网络凭据分享前需处理。
