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

`runtime_timing` 默认每 10 秒汇总 tick jitter 与 tick 实际耗时的 p50 / p95 / p99 / p999 / max。`runtime_stall` 字段统计实际 tick 间隔超出目标周期后严格超过 2 / 5 / 10 / 50 / 100 ms 的次数；超过 50 ms，或 SteamVR 输出队列满／关闭／写入失败的窗口为 warn，其余为 info。可通过 `--timing-window-ms` 调整窗口，退出时补发剩余样本。分析原因时需要结合 CPU 与线程调度，口径和量化精度见 [后端架构说明](rust-backend-api-architecture.zh-CN.md#tick-延迟统计)。同一 UDP 设备重新握手时，`device_connected` 的 `session` 递增，`preserve_calibration: true` 表示保留旋转校准。排查步骤见 [UDP 会话与校准](rust-backend-architecture.zh-CN.md#udp-会话与校准)。

`udp_ingress_backpressure` 按 `--summary-ms` 窗口汇总接收队列：`coalesced` 是被更新数据完整覆盖而合并掉的旧纯姿态包；`dropped_poses` 是合并后仍超过姿态容量而淘汰的最旧姿态包；`dropped_controls` 是控制队列满后丢弃的新控制包。只有合并时为 debug，发生容量丢包时为 warn；这三个计数的范围为应用接收队列，系统 UDP 缓冲区丢包由系统指标另行记录。接收策略见 [后端架构说明](rust-backend-api-architecture.zh-CN.md#执行和状态边界)。

设备快照里的 `sequence_gaps` 只表示接收器观察到的序号间隔，也会包含队列主动合并的旧包，需要结合队列指标判断缺口来源。

`runtime_timing` 还提供 `udp_queue_delay_ms`、`udp_batch_work_ms` 的 p50 / p95 / p99 / p999 / max，以及 `udp_datagrams`、`udp_batches`、`udp_budget_yields` 和队列合并／容量丢包计数。它们在 info 下可用，默认级别即可收集性能窗口。队列计数按 summary 窗口采集，可能归入相邻 timing 窗口。

`runtime_timing` 的 `api_live_snapshot_ms` 和 `api_live_snapshots` 测量 `Service::live()` 从构造开始到返回的墙钟分位数与次数。watch 发布和旧快照析构由后续路径执行。无 API 时为 null / 0。

同一记录中的 SteamVR 输出计数包括：

- `steamvr_output_batches_enqueued`：成功入队的批次。
- `steamvr_output_queue_full` / `steamvr_output_queue_closed`：非阻塞入队失败原因。
- `steamvr_output_batches_written`：IPC 已写完全部消息的批次，客户端消费由后续 SteamVR 链路执行。
- `steamvr_output_write_failed` / `steamvr_output_write_cancelled`：编码／写入／超时失败，以及连接结束取消正在发送的批次；可能已有部分消息写出。
- `steamvr_output_stale_batches`：重连后丢弃的旧会话批次。

异步入队与写完可能跨窗口，丢帧分析需要结合窗口边界、队列和 IPC 日志。UDP 与 SteamVR 分别统计。驱动输出使用 `try_send`，队列满时立即返回并累计计数。

`config_save_timing` 按每次有持久路径的保存记录 `total_ms`、`validation_ms`、
`serialization_ms`、`file_io_ms`、`sync_all_ms`、`serialized_bytes`、
`files_written` 和 `sync_all_calls`。`sync_all_ms` 为 `file_io_ms` 的子阶段。`outcome` 为 saved / unchanged / error；error 带错误类别，
字段记录错误类别与阶段数据。正常为 info，保存超过 4ms 或失败为 warn。相同内容
仍会构造 YAML 并读取比较，后返回 unchanged；记录只覆盖具有持久路径的实际保存。
运行期保存由独立的配置写入线程执行，带 `background: true`、保存 `revision`
和 `queue_delay_ms`；`total_ms` 从保存线程实际开始执行计量，队列等待单独统计。连续修改可能合并待写版本，
每个实际执行的保存才产生一条记录。磁盘失败还通过 `backend_error` 通知前端；
内存配置保留，保存成功确认仍等待落盘。正常退出会排空待写配置。
初始保存仍在追踪启动前同步完成，报告在 listening 后补发，耗时是启动保存的耗时。

Receiver freshness 和 TrackerPose 的 `pose_age_ms` 对 UDP 使用 socket 接收时刻，`pose_processing_age_ms` 保留主循环处理年龄，`pose_queue_delay_ms` 显示排队延迟。状态机与 replay 的时钟仍采用处理时刻，旧录制按原时刻回退。基准命令与测量结果见 [UDP 与 runtime 性能说明](rust-udp-runtime-performance.zh-CN.md)。

## 文件格式与轮转

桌面日志每行是一个 JSON 对象，包含秒级 `time`、毫秒级 `time_ms`、真实 `level`、来源 `source`（`gui` / `desktop` / `backend`）和 `args`。
`stdout` / `stderr` 表示进程输出通道，`level` 表示记录严重程度。桌面端解析后端提供的级别，兼容旧后端和 Java 常见级别标记。原生进程状态只落盘一次，前端收到后仅在开发者控制台显示。

桌面和 Overlay 日志按上表中的文件名保存，约 10 MiB 后轮转；历史文件以 `.1.log` 到 `.4.log` 结尾，例如 `gui-gpui.1.log` 或 `gui-tauri.1.log`。`.1` 最新，`.4` 最旧。最多保留五个文件，总量约 50 MiB，单条记录可能使文件略超上限。之前已有的超大日志在下一次写入时移入历史文件，通过文件轮转移动；这类历史文件在轮转淘汰前可能使总量超过约 50 MiB。

文件写入和轮转由一个加锁的持久文件句柄管理，串行处理前后端写入；每条记录刷新，便于异常退出后读取。日志过滤在后端序列化和前端 IPC 之前执行。

后端 `listen` 的诊断 JSON 由调用方序列化，写入线程将有界队列中的记录送到 stdout / stderr；队列满时记录 `logging_backpressure` 丢弃计数，正常退出排空队列。桌面日志由各自的同步持久句柄写入。线程与容量见 [API / runtime 边界](rust-backend-api-architecture.zh-CN.md#时间与配置更新)。journal 和 BVH 按各自文件契约写入。
