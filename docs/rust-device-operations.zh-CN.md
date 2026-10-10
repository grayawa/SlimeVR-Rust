# 设备发现、敲击分配与磁力计

设备设置使用原版 `vrconfig.yml` / `.yaml`。Tracker 身份、sensor 和会话契约见 [后端架构](rust-backend-architecture.zh-CN.md)，界面通信见 [API 通信契约](rust-backend-api-architecture.zh-CN.md)。

## 发现与连接

发现广播在没有在线 sensor 时每 10 秒发送一次，启动后先等待同样的安静窗口。固件主动握手会立即进入准入与响应流程，已连接设备使用接收器的保活周期。切换后端前退出占用同一端口的服务；设备等待后仍未出现时，重启对应追踪器。

固件参考：[connection.cpp](https://github.com/SlimeVR/SlimeVR-Tracker-ESP/blob/main/src/network/connection.cpp) 的 `searchForServer()` / `update()`。定制固件按其实际超时与握手规则验收。单个传感器的姿态年龄和设备心跳分别诊断，见 [API 与 runtime](rust-backend-api-architecture.zh-CN.md#执行和状态边界)。

## 敲击辅助分配

分配向导通过 `TapDetectionSettings.setupMode` 启用敲击选择。按原版逻辑检测两次敲击，返回 `TapDetectionSetupNotification`，其中包含稳定的 device ID 和实际 sensor ID，支持 sensor 0 与未分配 tracker。

分配模式暂停普通敲击复位。检测器按设备、sensor 和连接会话隔离；断线、加速度过期、模式／移动阈值改变会清理对应状态。移动背景阈值沿用现有 TapDetector。模式写入原版 `tapDetection.setupMode`，与原服务一样由前端切回普通模式。

## 磁力计控制

全局设置使用 `server.useMagnetometerOnAllTrackers`，单 tracker 偏好使用原版 `trackers["udp://MAC/sensor"].shouldHaveMagEnabled`。实际启用条件为全局开关与单 tracker 偏好的交集；未设置的单 tracker 偏好默认开启，全局默认关闭。删除设备同时清除其偏好，保留其他 tracker 来源。

SensorConfig 位语义：bit 1 表示支持，bit 0 表示启用。SolarXR TrackerInfo 返回 NOT_SUPPORTED／DISABLED／ENABLED，使原有磁力计控件能够显示。

命令沿用 UDP 包 25：12 字节头、sensor byte、配置类型 u16 大端和布尔 byte，MAG_ENABLED 类型为 1；等待包 24 ACK 后更新实际报告状态并回复前端。未 ACK 的请求最多等待 10 秒，断线／重启报告未确认；希望配置保留在 YAML，并在设备新会话中重新下发。超时后，同一 sensor 等待旧 ACK 或重连，按当前会话隔离 ACK 的确认范围。协议 ACK 的内容为确认类型与传感器，实际状态通过 SensorInfo 报告；重复 ACK 的行为按真实固件验收。

journal 版本 3 保存设备命令，回放验证相同命令、实际 UDP 输出和 ACK 后状态；回放器同时读取版本 1、2。

## 验证与实机检查

自动检查覆盖真实 UDP / CLI 握手、sensor 0、双 sensor、敲击分配和 YAML 保存；设备命令检查覆盖 ACK 前等待、超时、迟到 ACK、重连重新下发、非法命令组及 journal 回放。

实机按 [统一清单](rust-unified-hardware-test.zh-CN.md) 检查发现、批准、佩戴、敲击分配、磁力计 ACK / 超时与重连，记录固件、sensor、操作和日志时间。设备实际融合、温度校准与板端旋转按对应固件测量；服务端使用融合后的四元数。
