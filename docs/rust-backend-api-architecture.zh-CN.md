# Rust 后端 API 模块结构

日期：2026-10-07。本次将原来 2,525 行的 `api/mod.rs` 拆成按职责组织的模块。入口只保留 18 行模块声明和公共导出，调用方继续使用 `api::Service`、`api::FrontendConfig`、`api::Request`、`api::Wire`、`api::LiveState` 和 `api::serve`。

## 文件职责

以下路径相对于 `server-rust/crates/slimevr-server/src/api/`。

| 文件 / 目录                    | 职责                                                                 |
| ------------------------------ | -------------------------------------------------------------------- |
| `mod.rs`                       | 声明模块，保持公共入口兼容                                           |
| `config.rs`                    | `FrontendConfig` 默认值、限制校验、调用原有 YAML 读写                |
| `types.rs`                     | 跨任务的请求、响应和只读实时快照类型                                 |
| `transport/mod.rs`             | WebSocket 监听、连接数限制、握手、事件循环和后端信息                 |
| `transport/session.rs`         | 单个连接的 data feed、pub/sub、串口日志和配网订阅；RPC 转交与写出    |
| `service/mod.rs`               | 应用状态、初始化、配置提交、绑定变化后的观测恢复、快照生成           |
| `service/sources.rs`           | 外部追踪来源、SteamVR 头显 / 手柄输入及来源所有权                    |
| `service/notifications.rs`     | SteamVR 自动共享、连接状态和驱动管理结果通知                         |
| `service/calibration.rs`       | 快捷键、延迟暂停、完整 / 航向 / 安装方向重置的计时和进度             |
| `service/devices.rs`           | 串口、配网、固件更新和设备控制的事件协调                             |
| `service/recording.rs`         | AutoBone 保存 / 处理结果和 BVH 录制生命周期                          |
| `service/tick.rs`              | 解算后的检查清单、敲击分配、设备 ID、配置保存和录制采样              |
| `service/legacy.rs`            | 原版 JSON WebSocket 的 `config` / `pos` / `action` 与姿态注入        |
| `service/rpc/mod.rs`           | SolarXR 校验、请求日志、批次限制、串口占用保护、按功能分发和错误返回 |
| `service/rpc/configuration.rs` | 软件与算法设置、身体比例和快捷键请求                                 |
| `service/rpc/trackers.rs`      | 设备批准 / 删除、追踪器分配和磁力计请求                              |
| `service/rpc/calibration.rs`   | 重置、暂停、StayAligned、腿部临时设置和身高校准请求                  |
| `service/rpc/hardware.rs`      | 串口命令、Wi-Fi 配网和固件请求                                       |
| `service/rpc/recording.rs`     | AutoBone 和 BVH 请求                                                 |
| `service/rpc/system.rs`        | 心跳、驱动启用、检查清单、状态、VRChat、overlay 和服务端信息         |

既有 `protocol.rs` 编码、`settings.rs` 转换、`device_control.rs` 控制器、`diagnostics.rs` 检查清单、`status.rs` 状态系统、`vrchat.rs` 平台配置和 `pubsub.rs` 主题注册表继续负责各自的功能。

## 执行和状态边界

后端的 `main` 使用两个工作线程的 Tokio runtime。`listen` 是由主线程
`block_on` 执行的根 future，不作为可迁移的任务启动，因此 Receiver、
PoseEngine 和 Service 仍由同一个线程按顺序更新。`tokio::spawn` 启动的
WebSocket、SteamVR IPC、OSC 和 UDP 接收任务在两个工作线程上执行；它们
通过已有的有界命令队列、watch 快照和 broadcast 通知与主循环交换数据。
串口和 HID 沿用各自的设备线程，AutoBone 等阻塞计算沿用后台任务。

`runtime/ingress.rs` 独立读取追踪器 UDP，队列最多保存 128 个数据包，
保持已接收数据的顺序。主循环过载时，队列满后不等待，也不继续复制数据包，
而是丢弃新包并汇总为 `udp_ingress_backpressure` 日志。这样能限制内存和
积压，但不能保证电脑完全满载时不丢包，也没有改变姿态算法本身。

诊断日志由 `slimevr-log-writer` 线程写 stdout / stderr。调用方仍负责
序列化一条 JSON，但不在实时循环里执行管道写入。队列同时限制为 512 条
和 4 MiB（包含正在写的一条），满时丢弃诊断日志，写入恢复后报告
`logging_backpressure` 及丢弃数量。正常退出会排空队列；管道一直堵塞时，
退出最多等两秒，避免被日志消费者拖住。错误和警告仍走 stderr。

这次没有异步化配置提交、UDP replay journal 和 BVH 文件写入。这些输出
需要保持成功确认与持久化的关系，不能沿用诊断日志的丢弃策略；下一步应
使用独立、有序的文件工作队列，并把写入结果送回状态所有者。

```mermaid
flowchart LR
    UI[前端 WebSocket] <--> Session[每个连接的 Session]
    Native[SteamVR 原生 RPC] --> Queue[有界 Request 队列]
    Session --> Queue
    Queue --> Runtime[runtime 单一状态所有者]
    Runtime --> Service[Service 与领域处理]
    Runtime --> Engine[PoseEngine]
    Service --> Engine
    Runtime --> Snapshot[watch 实时快照]
    Snapshot --> Session
    Service --> Events[broadcast 通知]
    Events --> Session
```

`Session` 持有连接自己的订阅、发送状态和主题身份，不持有 `PoseEngine`。领域处理仍通过同一个 `Service` 修改配置、记录场景变化和协调控制器；这些调用在原来的 runtime 循环内执行。拆分不新增算法状态锁，不改变 UDP 接收或解算调度，也不把每种 RPC 变成独立异步任务。

`Service` 的内部字段保留在 `service/mod.rs`，其他文件通过同一类型的 `impl Service` 实现对应职责。跨领域的私有方法仅在 `service` 范围可见，连接侧订阅状态则由独立的 `Session` 结构体封装。

## 保留的通信行为

- SolarXR FlatBuffers、旧 JSON WebSocket 和原生 RPC 的公开类型及字段保持兼容。
- 请求按批次内原顺序执行，直接响应保留事务号；异步广播继续走原来的事件通道。
- 批次上限仍为 32。超量批次在执行前拒绝；普通批次某条请求失败时，停止后续请求，已经执行的操作保留，返回错误和当前设置。
- 连接上限、帧大小、握手 / RPC / 发送超时以及 data feed 最小间隔不变。
- pub/sub 的订阅按连接隔离，排除向发送者回送；串口和配网通知按连接订阅过滤。
- 原配置文件、校准计时、AutoBone / BVH 保存和此前的 UDP 重连校准修复继续生效。

## 修改入口与验证

新增 RPC 时，在 `service/rpc/mod.rs` 注册所属领域，并在对应领域文件中实现；请求校验和错误包装保留在统一入口。修改设置转换优先看 `settings.rs`；修改具体协议编码优先看 `protocol.rs`；修改 UDP 或算法行为仍分别进入接收层和 `slimevr-core`。

本次补充了三项跨领域批次契约测试，在拆分前后的实现上均通过，覆盖响应顺序 / 事务号、广播和 YAML 保存、失败后的执行边界、超量批次无副作用。两项真实 WebSocket 测试覆盖连接初始化、心跳、轮询和定时 data feed，以及多连接 pub/sub 隔离和发送者排除。

最终验证：126 项后端测试通过；格式检查及 Linux / Windows x64 目标的全目标 Clippy（`-D warnings`）通过。六设备 UDP / WebSocket / SteamVR Unix IPC 模拟联调对照拆分前后的正常、头显中断、头显恢复和后端停顿重连四个阶段，旋转、计算点位、校准标记和会话号逐项一致。这些检查不替代 Windows / SteamVR 实机验收。
