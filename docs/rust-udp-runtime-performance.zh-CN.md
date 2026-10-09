# UDP 与 runtime 性能验证

2026-10-08。实现提交 `110dc2de`；完整 runtime 对照基线为合入 PR #10 后的
`6f936018`。本轮修复双时钟诊断、重复解析、配置高频导出与重配，并限制 UDP
在两次调度之间的工作量。没有改变传感器停更时的缓存姿态策略。

## 队列／解析微基准

在仓库根目录运行：

```sh
cargo test --manifest-path server-rust/Cargo.toml -p slimevr-server --release --lib runtime::ingress::bench::benchmark_udp_queue_and_cached_parse --locked -- --ignored --nocapture
```

这个用例默认 ignored，避免在普通 CI 中用机器速度决定通过或失败。每轮
30,720 个包、六个来源、各两个传感器，覆盖单旋转、双传感器与加速度 bundle、
控制消息较多的 bundle，包含正常交付和模拟 100ms 积压。每种条件重复三次，
交替执行保留解析结果与重新解析的路径。断言两条路径的 Receiver 状态一致，
输出每包 enqueue / dispatch 的 p50 / p95 / p99 / p999 / max、整轮时间与
选中、合并、容量丢弃数量。

enqueue 包含字节复制、完整解析、BTreeSet 分类、Mutex 加锁和 Queue::push；
dispatch 包含锁内 pop、接收器处理和 effects 构造。积压用例还执行 BTreeMap
覆盖判定。这里使用无竞争的真实 Mutex；不含网络、PoseEngine、Service 与
调度预算，锁竞争和这些后续工作由下一个真实 runtime 基准覆盖。

`raw_reparse_reference` 在同一队列中模拟再次解析，隔离第二次解析的成本，
不是 PR #10 整个实现的逐字复制。下表是三轮 dispatch p95 的中位数，单位 µs：

| 输入              | 正常：重解析 | 正常：复用 | 积压：重解析 | 积压：复用 |
| ----------------- | ------------ | ---------- | ------------ | ---------- |
| 单旋转            | 0.210        | 0.160      | 0.250        | 0.211      |
| 双传感器 bundle   | 0.411        | 0.291      | 0.501        | 0.401      |
| 控制较多的 bundle | 0.601        | 0.561      | 0.481        | 0.391      |

复用降低了 dispatch 成本，但积压情况下整轮时间也受分类和合并主导：
单旋转整轮中位数为 7.540 → 7.864ms，双传感器 bundle 为
11.333 → 11.761ms。不能只凭 dispatch 变快就断言总成本下降。

## 真实 UDP 与姿态 tick

先编译 release 后端，然后分别运行空载和竞争 CPU 的用例：

```sh
cargo build --manifest-path server-rust/Cargo.toml --release --locked --bin slimevr-server
python server-rust/tools/benchmark-runtime.py --backend server-rust/target/release/slimevr-server --duration 10 --cpu-workers 0 --output idle.json
python server-rust/tools/benchmark-runtime.py --backend server-rust/target/release/slimevr-server --duration 10 --cpu-workers 2 --output loaded.json
```

Windows 把 backend 路径改为 `slimevr-server.exe`。`--cpu-workers` 指定额外的
持续计算进程数，按电脑的可用逻辑核心数选择；默认 0。负载进程在结束时停止，
异常退出也会清理。`--devices` 默认 6，`--sensors` 默认 2，`--rate` 默认每设备
700 包／秒，即总计 4,200 包／秒。

脚本创建临时 vrconfig.yml、身体分配和禁用的热键，使用独立 loopback 端口，
以原 UDP 握手、SensorInfo、紧凑旋转＋加速度包驱动真实后端，启用 4ms pose
tick 和 API Service。它持续读取 stdout / stderr，保持 info 日志级别，收集
runtime_timing；不连接 SteamVR，也没有 HMD 输入。JSON 保留实际发送速率、
间隔样本数、所有分位数、stall 与新版本的 UDP 指标。Linux cgroup v2 下还
记录容器 CPU 配额与整个容器的平均利用率，后者不是后端单进程的 CPU 占用。

本次环境为 Linux x86_64、Rust 1.99.0、可见三个逻辑 CPU、cgroup 配额两个
CPU。基线／修改版各做三轮 10 秒测试，分别使用 0 / 2 个压力进程，共十二轮。
实际发送速率中位数约 4,199.7 包／秒；压力条件下容器配额平均利用率约 100%。
每轮主窗口有 2,416–2,500 个实际 tick 间隔；没有把错过的 tick 虚构成样本。

下表分位数取三轮主窗口的中位数；max 取三轮观测到的最大值，单位 ms：

| 条件           | jitter p50 | p95   | p99   | p999   | max     | tick work p95 |
| -------------- | ---------- | ----- | ----- | ------ | ------- | ------------- |
| PR #10，无压力 | 0.208      | 0.468 | 0.567 | 1.943  | 6.516   | 0.242         |
| 修改版，无压力 | 0.222      | 0.460 | 0.547 | 1.615  | 2.769   | 0.262         |
| PR #10，满配额 | 0.284      | 3.567 | 5.183 | 8.223  | 104.191 | 0.269         |
| 修改版，满配额 | 0.286      | 2.663 | 6.815 | 11.167 | 16.942  | 0.235         |

满载的 p95 与 tick work p95 较低，但 p99 / p999 较高；这组结果没有证明
尾延迟整体改善，也不足以断言变化来自某个数据结构。最大值涉及一次偶发
停顿，不能单独作为改进结论。两版都受操作系统调度影响，平均容器利用率中
还包括发包器和其他进程，不能据此比较后端单进程 CPU。

修改版满载时 UDP 等待 p99 的三轮中位数为 0.273ms，批处理耗时 p99 为
0.007ms；批处理 max 的三轮中位数为 0.638ms。预算只在完整 datagram 边界
检查，单包工作、文件写入和线程停调仍可能超过预算。物理 Windows 电脑、
Wi-Fi、HMD、SteamVR 输出与 VRChat 尚需实测。

原始记录和环境信息见 [测量数据](../server-rust/benchmarks/udp-runtime-2026-10-08.json)。
后续优化 BTreeSet / BTreeMap 或 Mutex 前应保留这组对照，并在目标 Windows
机器上测量锁竞争和后端 CPU，不能把分位数的单次变化直接当成性能回归。

## API 快照共享与配置写入（2026-10-09）

运行隔离的快照构造微基准：

```sh
cargo test --manifest-path server-rust/Cargo.toml -p slimevr-server --release --lib api::service::live_bench::benchmark_live_snapshot_sharing --locked -- --ignored --nocapture
```

六台设备、每台两个传感器、12 个已分配节点，使用 `vrconfig-v15.yml` 测试
配置。每种路径各做三轮、每轮 20,000 次，交替先后顺序。参考路径重建旧
`Service::live()` 的全量克隆字段集合；共享路径调用新的 `Service::live()`。
两者都包含构造和析构，不包含解算、网络、watch 发布与锁竞争。这与日志中
不含析构的 `api_live_snapshot_ms` 测量边界不同，不应直接比较绝对值。

下表分位数取三轮中位数，max 取三轮最大观测值，单位 µs：

| 路径         | p50    | p95    | p99     | p999    | max      |
| ------------ | ------ | ------ | ------- | ------- | -------- |
| 全量克隆参考 | 25.869 | 40.651 | 113.751 | 460.633 | 6103.737 |
| 共享快照     | 0.711  | 0.771  | 1.002   | 14.793  | 253.661  |

原始记录见 [快照测量数据](../server-rust/benchmarks/live-snapshot-2026-10-09.json)。
这个微基准证明复制开销降低，不证明 SteamVR / VRChat 的整体 p99 或 p999
已经改善。墙钟尾延迟包含系统抢占，本轮测试期间环境还有其他测试进程。
姿态 tick 改为创建一个共享 `Arc`，这部分新增成本不在快照微基准内。
设备动态读数仍复制；外部追踪小表与 SteamVR 状态仍按值复制。

配置保存使用独立 OS 线程，保留原子的 YAML 备份、`sync_all()` 和替换逻辑。
慢磁盘测试用阻塞的写入函数验证：首个写入阻塞期间，生产方继续提交 99 次
配置并推进 PoseEngine；队列只有正在写的版本和最新待写版本，释放后仅
写入 revision 1 与 100，退出等待完成。额外测试验证保存失败的通知、失败后
保留内存配置，以及客户端回复等待持久化 revision。阻塞替身用于稳定验证
调度边界，不用于预测实际磁盘耗时。启动保存、离线工具、BVH 与 replay
journal 仍使用各自原来的同步接口。
