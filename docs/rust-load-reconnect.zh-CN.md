# 调度停顿后的 UDP 重连与校准

已知 UDP 设备重新握手时保留完整、安装方向与 yaw 校准。接收会话递增，动态样本、滤波、速度和接触历史重新建立；首条旋转直接初始化滤波器。该规则适用于同一稳定身份的 UDP 来源。

## 会话契约

首次设备从初始校准状态开始。重连后接收器按新会话和当前地址接受样本。HID、SteamVR、OSC 等来源按自身会话规则重建。旧 journal 缺少 `preserve_calibration` 时使用默认 false。

UDP 握手描述网络会话，设备实际重启、佩戴变化或旋转参考变化后，需要使用者重新校准。原版参考为 `TrackersUDPServer.setUpNewConnection` 的设备与 Tracker 复用路径。

## 受控验证

回归覆盖 MAC 保持、端口变化、校准保留、旧会话拒绝、新姿态恢复、平滑 / 预测首条样本与旧录制字段。

六台模拟设备通过真实 UDP、SolarXR WebSocket 和 SteamVR Unix IPC 联调。完成完整与安装方向重置后暂停后端 3.3 秒，设备以测试设置的 2 秒无保活触发重新握手。恢复后六台校准保留，左脚恢复为约 `(-0.13, 0.12, 0.10)` 米，六个输出状态正常。HMD 单独中断 0.8 秒时，世界锚点暂时失效，UDP 校准保留。

单逻辑 CPU 竞争用例中，后端以低调度优先级运行，观测到约 1.4 秒 tick 间隔和六设备重新握手，负载结束后校准与位置恢复。这些受控用例验证停顿后的恢复路径；目标 Windows / VRChat 的实时表现按相同负载和设备配置另行测量。

## 实机排查

完成正常校准后复现 CPU 密集场景，记录异常时间、负载下降后的恢复情况和日志。先使用默认 info，需要姿态快照时使用调试脚本。

核对 `runtime_timing` 的 jitter 分位数、stall 阈值，以及同一 `device_key` 的 `device_connected` 会话递增与 `preserve_calibration: true`。其他定位线索包括 SteamVR 输入、输出队列、IPC 和系统调度。持续追踪需要主机为接收、解算与输出提供足够执行时间。

统计口径见 [API 架构](rust-backend-api-architecture.zh-CN.md#tick-延迟统计)，日志步骤见 [日志说明](rust-logging.zh-CN.md)。
