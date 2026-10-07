# Rust / Tauri 日志级别与收集方法

默认日志级别为 `info`，前端、桌面进程和由桌面启动的 Rust 后端使用同一级别。
日志仍保存在 `%APPDATA%\dev.slimevr.SlimeVR\logs`，无需改动 `vrconfig.yml`，也不新增 JSON 配置文件。

| 级别    | 内容                                                                               |
| ------- | ---------------------------------------------------------------------------------- |
| `error` | 后端启动失败、异常退出、操作失败、无法解析服务端响应                               |
| `warn`  | WebSocket 异常关闭、连接处理失败、设备超时、损坏数据包、UDP 发送失败、后端调度停顿 |
| `info`  | 启动与停止、连接和重连、设备连接与状态、SteamVR 状态变更、重置完成、BVH 开始与保存 |
| `debug` | 完整姿态与设备快照、API 请求类型和事务号                                           |
| `trace` | 逐样本数据、遥测、被忽略的数据包、API 心跳请求                                     |

指定级别会同时保留比它更严重的记录。默认 `info` 不生成完整快照及逐样本日志；正常姿态计算、前端数据订阅、SteamVR 输出和回放录制照常运行。

## 给使用者的操作步骤

1. 日常使用：双击 `SlimeVR.exe` 或 `Start-SlimeVR.cmd`。
2. 遇到问题时先完全退出 SlimeVR，包括托盘里的程序。
3. 双击 `Start-SlimeVR-Debug.cmd`，再重复刚才出现问题的操作，记下时间。
4. 按 `Win + R`，粘贴 `%APPDATA%\dev.slimevr.SlimeVR\logs`，按回车。
5. 把这个文件夹内的日志一起压缩，附上发生时间、当时操作和问题表现。
6. 完全退出后，用正常方式重新启动，即恢复默认日志级别。

已有实例运行时，双击调试脚本只会唤醒它，不会更改运行中的日志级别。若连接的是单独启动的后端，它也需要以相应级别重新启动。

## 开发与深入排查

```powershell
.\SlimeVR.exe --log-level debug
.\SlimeVR.exe --log-level trace
.\slimevr-server.exe listen --api-bind 127.0.0.1:21110 --log-level debug
```

也支持环境变量 `SLIMEVR_LOG_LEVEL=debug`。优先级为命令行参数、环境变量、默认 `info`。无显式级别和环境变量时，后端原来的 `--events` 等效于 `trace`。非法级别报错，不会静默回退。

独立后端 `listen` 输出 JSON Lines，新增 `level` 字段，原来的 `type` 与数据字段保留；`warn/error` 输出到 stderr，其他级别输出到 stdout。
`replay`、`decode`、`solve`、`solve-recording`、`autobone` 和刷写工作进程的机器输出保持原样，不受日志过滤影响。

API 调试日志记录请求类型和事务号，不记录请求正文。重置只在完成时写常规日志，不逐帧写倒计时。

`runtime_timing` 默认每 10 秒汇总 tick jitter 与 tick 实际耗时的 p50 / p95 / p99 / p999 / max。`runtime_stall` 字段统计实际 tick 间隔超出目标周期后严格超过 2 / 5 / 10 / 50 / 100 ms 的次数；超过 50 ms 的窗口为 warn，其余为 info。可通过 `--timing-window-ms` 调整窗口，退出时补发剩余样本。这些指标不能单独证明 CPU 饱和，口径和量化精度见 [后端架构说明](rust-backend-api-architecture.zh-CN.md#tick-延迟统计)。同一 UDP 设备重新握手时，`device_connected` 的 `session` 递增，`preserve_calibration: true` 表示保留旋转校准。排查步骤见 [重连校准修复](rust-load-reconnect.zh-CN.md)。

## 文件格式与轮转

桌面日志每行是一个 JSON 对象，包含秒级 `time`、毫秒级 `time_ms`、真实 `level`、来源 `source`（`gui` / `desktop` / `backend`）和 `args`。
`stdout` / `stderr` 是进程输出通道，已不再充当日志级别。桌面端解析后端提供的级别，兼容旧后端和 Java 常见级别标记。原生进程状态只落盘一次，前端收到后仅在开发者控制台显示。

当前文件为 `gui-tauri.log`。约 10 MiB 后轮转成 `gui-tauri.1.log`，之后顺延到 `.4.log`；`.1` 最新，`.4` 最旧。最多保留五个文件，总量约 50 MiB，单条记录可能使文件略超上限。之前已有的超大日志在下一次写入时移入历史文件，不会整体读入内存；这类历史文件在轮转淘汰前可能使总量超过约 50 MiB。

文件写入和轮转由一个加锁的持久文件句柄管理，避免前后端并发写入交错；每条记录刷新，便于异常退出后读取。日志过滤在后端序列化和前端 IPC 之前执行。
