# GPUI / Tauri / 后端日志收集

默认日志级别为 `info`，前端、桌面进程和由桌面启动的 Rust 后端使用同一级别。
级别由启动参数或环境变量选择。

## 日志位置

| 界面     | 当前日志                                     |
| -------- | -------------------------------------------- |
| GPUI     | 应用数据目录中的 `logs/gui-gpui.log`         |
| Tauri    | 应用数据目录中的 `logs/gui-tauri.log`        |
| Overlay  | 应用数据目录中的 `logs/overlay/gui-gpui.log` |
| 独立后端 | stdout / stderr，由启动方式保存              |

Windows 应用数据目录为 `%APPDATA%\dev.slimevr.SlimeVR`；Linux GPUI 为 `${XDG_CONFIG_HOME:-$HOME/.config}/dev.slimevr.SlimeVR`。其他 Tauri 平台路径见 [宿主 README](../gui/README.tauri.md)。

## 日志级别

| 级别    | 内容                                                                               |
| ------- | ---------------------------------------------------------------------------------- |
| `error` | 后端启动失败、异常退出、操作失败、无法解析服务端响应                               |
| `warn`  | WebSocket 异常关闭、连接处理失败、设备超时、损坏数据包、UDP 发送失败、后端调度停顿 |
| `info`  | 启动与停止、连接和重连、设备连接与状态、SteamVR 状态变更、重置完成、BVH 开始与保存 |
| `debug` | 完整姿态与设备快照、API 请求类型和事务号                                           |
| `trace` | 逐样本数据、遥测、被忽略的数据包、API 心跳请求                                     |

指定级别会同时保留比它更严重的记录。完整快照使用 debug，逐样本使用 trace。日志过滤作用于诊断输出，算法、订阅、SteamVR 与 journal 按各自生命周期运行。

## 收集日志

### Windows

1. 日常使用：双击 `SlimeVR.exe` 或 `Start-SlimeVR.cmd`。
2. 遇到问题时先完全退出 SlimeVR，包括托盘里的程序。
3. 双击 `Start-SlimeVR-Debug.cmd`，再重复刚才出现问题的操作，记下时间。
4. 按 `Win + R`，粘贴 `%APPDATA%\dev.slimevr.SlimeVR\logs`，按回车。
5. 把这个文件夹内的日志一起压缩，附上发生时间、当时操作和问题表现。
6. 完全退出后，用正常方式重新启动，即恢复默认日志级别。

已有实例运行时，双击调试脚本唤醒当前实例；级别在该实例启动时确定。若连接的是单独启动的后端，它也需要以相应级别重新启动。

### Linux GPUI

完全退出已有实例后执行 `./Start-SlimeVR.sh --log-level debug`，复现问题并记下时间。退出后压缩应用数据目录中的整个 `logs/` 文件夹，附上操作与现象。日常启动采用默认 info。独立后端与 Overlay 按自己的启动参数选择级别。

## 开发与深入排查

```powershell
.\SlimeVR.exe --log-level debug
.\SlimeVR.exe --log-level trace
.\slimevr-server.exe listen --api-bind 127.0.0.1:21110 --log-level debug
```

也支持环境变量 `SLIMEVR_LOG_LEVEL=debug`。优先级为命令行参数、环境变量、默认 `info`。无显式级别和环境变量时，后端 `--events` 等效于 `trace`。非法级别返回参数错误。

独立后端 `listen` 输出 JSON Lines，包含 `level`、`type` 和对应数据字段；`warn/error` 输出到 stderr，其他级别输出到 stdout。
`replay`、`decode`、`solve`、`solve-recording`、`autobone` 和刷写工作进程的机器输出保持原样，使用各命令的独立输出契约。

API 调试日志记录请求类型和事务号。常规重置日志记录完成结果，倒计时通过进度消息更新界面。

## 性能与连接排查入口

默认 info 日志包含性能窗口，记录问题发生时间后对照以下入口：

| 现象                         | 记录与说明                                                                                                                                                                                                       |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 姿态冻结、CPU 满载或点位异常 | `runtime_timing` 的 jitter / work / stall，定义见 [tick 延迟统计](rust-backend-api-architecture.zh-CN.md#tick-延迟统计)                                                                                          |
| UDP 积压、序号缺口或旧姿态   | `udp_ingress_backpressure`、队列等待与姿态年龄，见 [接收边界](rust-backend-api-architecture.zh-CN.md#执行和状态边界) 和 [时间语义](rust-backend-api-architecture.zh-CN.md#时间与配置更新)                        |
| SteamVR 跳帧或设置保存卡顿   | 输出队列 / IPC 计数与 `config_save_timing`，见 [快照、保存与输出](rust-backend-api-architecture.zh-CN.md#快照保存与-steamvr-输出诊断)                                                                            |
| 设备重新握手或前端断联       | `device_connected` 的 session / 校准保留标记，以及 `api_connection_error`；见 [UDP 会话](rust-backend-architecture.zh-CN.md#udp-会话与校准) 和 [前端连接](rust-backend-api-architecture.zh-CN.md#前端连接与通知) |

性能排查结合系统 CPU、线程调度与同一时段的输入 / 输出记录；复现命令和测量条件见 [runtime 基准](rust-udp-runtime-performance.zh-CN.md)。

## 文件格式与轮转

桌面日志每行是一个 JSON 对象，包含秒级 `time`、毫秒级 `time_ms`、真实 `level`、来源 `source`（`gui` / `desktop` / `backend`）和 `args`。
`stdout` / `stderr` 表示进程输出通道，`level` 表示记录严重程度。桌面端解析后端提供的级别，兼容旧后端和 Java 常见级别标记。原生进程状态只落盘一次，前端收到后仅在开发者控制台显示。

桌面和 Overlay 日志按上表中的文件名保存，约 10 MiB 后轮转；历史文件以 `.1.log` 到 `.4.log` 结尾，例如 `gui-gpui.1.log` 或 `gui-tauri.1.log`。`.1` 最新，`.4` 最旧。最多保留五个文件，总量约 50 MiB，单条记录可能使文件略超上限。之前已有的超大日志在下一次写入时移入历史文件，通过文件轮转移动；这类历史文件在轮转淘汰前可能使总量超过约 50 MiB。

文件写入和轮转由一个加锁的持久文件句柄管理，串行处理前后端写入；每条记录刷新，便于异常退出后读取。日志过滤在后端序列化和前端 IPC 之前执行。

后端 `listen` 的诊断 JSON 由调用方序列化，写入线程将有界队列中的记录送到 stdout / stderr；队列满时记录 `logging_backpressure` 丢弃计数，正常退出排空队列。桌面日志由各自的同步持久句柄写入。线程与容量见 [API / runtime 边界](rust-backend-api-architecture.zh-CN.md#时间与配置更新)。journal 和 BVH 按各自文件契约写入。
