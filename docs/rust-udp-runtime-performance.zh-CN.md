# UDP 与 runtime 性能验证

2026-10-08。实现提交 `110dc2de`；完整 runtime 对照基线为合入 PR #10 后的
`6f936018`。被测实现使用双时钟诊断、共享解析结果、配置 revision / dirty 标志与有预算的 UDP 调度。传感器停更按缓存姿态可用性规则处理。

## 队列／解析微基准

在仓库根目录运行：

```sh
cargo test --manifest-path server-rust/Cargo.toml -p slimevr-server --release --lib runtime::ingress::bench::benchmark_udp_queue_and_cached_parse --locked -- --ignored --nocapture
```

该性能用例通过显式 `--ignored` 运行，按环境报告墙钟测量。每轮
30,720 个包、六个来源、各两个传感器，覆盖单旋转、双传感器与加速度 bundle、
控制消息较多的 bundle，包含正常交付和模拟 100ms 积压。每种条件重复三次，
交替执行保留解析结果与重新解析的路径。断言两条路径的 Receiver 状态一致，
输出每包 enqueue / dispatch 的 p50 / p95 / p99 / p999 / max、整轮时间与
选中、合并、容量丢弃数量。

enqueue 包含字节复制、完整解析、BTreeSet 分类、Mutex 加锁和 Queue::push；
dispatch 包含锁内 pop、接收器处理和 effects 构造。积压用例还执行 BTreeMap
覆盖判定。这里使用无竞争的真实 Mutex；测量边界为队列 / 解析，网络、PoseEngine、Service、调度预算与锁竞争由下一个真实 runtime 基准覆盖。

`raw_reparse_reference` 在同一队列中模拟再次解析，隔离第二次解析的成本，
该参考用于单独测量第二次解析。下表是三轮 dispatch p95 的中位数，单位 µs：

| 输入              | 正常：重解析 | 正常：复用 | 积压：重解析 | 积压：复用 |
| ----------------- | ------------ | ---------- | ------------ | ---------- |
| 单旋转            | 0.210        | 0.160      | 0.250        | 0.211      |
| 双传感器 bundle   | 0.411        | 0.291      | 0.501        | 0.401      |
| 控制较多的 bundle | 0.601        | 0.561      | 0.481        | 0.391      |

复用降低了 dispatch 成本，但积压情况下整轮时间也受分类和合并主导：
单旋转整轮中位数为 7.540 → 7.864ms，双传感器 bundle 为
11.333 → 11.761ms。总成本需要结合 enqueue、dispatch 与合并整轮时间判断。

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
runtime_timing；测试输入为 UDP 设备，输出为 runtime 诊断。JSON 保留实际发送速率、
间隔样本数、所有分位数、stall 与新版本的 UDP 指标。Linux cgroup v2 下还
记录容器 CPU 配额与整个容器的平均利用率，该范围包含容器中的全部进程。

测量环境为 Linux x86_64、Rust 1.99.0、可见三个逻辑 CPU、cgroup 配额两个
CPU。基线／修改版各做三轮 10 秒测试，分别使用 0 / 2 个压力进程，共十二轮。
实际发送速率中位数约 4,199.7 包／秒；压力条件下容器配额平均利用率约 100%。
每轮主窗口有 2,416–2,500 个实际 tick 间隔；样本按实际 tick 间隔累计。

下表分位数取三轮主窗口的中位数；max 取三轮观测到的最大值，单位 ms：

| 条件           | jitter p50 | p95   | p99   | p999   | max     | tick work p95 |
| -------------- | ---------- | ----- | ----- | ------ | ------- | ------------- |
| PR #10，无压力 | 0.208      | 0.468 | 0.567 | 1.943  | 6.516   | 0.242         |
| 修改版，无压力 | 0.222      | 0.460 | 0.547 | 1.615  | 2.769   | 0.262         |
| PR #10，满配额 | 0.284      | 3.567 | 5.183 | 8.223  | 104.191 | 0.269         |
| 修改版，满配额 | 0.286      | 2.663 | 6.815 | 11.167 | 16.942  | 0.235         |

满载结果中的 p95 与 tick work p95 较低，p99 / p999 较高。结论需要结合各分位数、偶发停顿、重复轮次和系统调度；具体数据结构的成本由隔离基准测量。容器利用率覆盖发包器和其他进程，后端单进程 CPU 需单独采集。

修改版满载时 UDP 等待 p99 的三轮中位数为 0.273ms，批处理耗时 p99 为
0.007ms；批处理 max 的三轮中位数为 0.638ms。预算只在完整 datagram 边界
检查，单包工作、文件写入和线程停调仍可能超过预算。物理 Windows 电脑、
Wi-Fi、HMD、SteamVR 输出与 VRChat 尚需实测。

原始记录和环境信息见 [测量数据](../server-rust/benchmarks/udp-runtime-2026-10-08.json)。
后续优化 BTreeSet / BTreeMap 或 Mutex 前应保留这组对照，并在目标 Windows
机器上测量锁竞争和后端 CPU，使用多轮相同环境结果评估分位数变化。

## API 快照共享与配置写入（2026-10-09）

运行隔离的快照构造微基准：

```sh
cargo test --manifest-path server-rust/Cargo.toml -p slimevr-server --release --lib api::service::live_bench::benchmark_live_snapshot_sharing --locked -- --ignored --nocapture
```

六台设备、每台两个传感器、12 个已分配节点，使用 `vrconfig-v15.yml` 测试
配置。每种路径各做三轮、每轮 20,000 次，交替先后顺序。参考路径构造全量克隆字段集合，共享路径调用 `Service::live()`。
两条路径都计量构造与析构。解算、网络、watch 发布和锁竞争由 runtime 指标或相应基准测量。日志的 `api_live_snapshot_ms` 从构造开始到返回结束，比较时需对齐计时边界。

下表分位数取三轮中位数，max 取三轮最大观测值，单位 µs：

| 路径         | p50    | p95    | p99     | p999    | max      |
| ------------ | ------ | ------ | ------- | ------- | -------- |
| 全量克隆参考 | 25.869 | 40.651 | 113.751 | 460.633 | 6103.737 |
| 共享快照     | 0.711  | 0.771  | 1.002   | 14.793  | 253.661  |

原始记录见 [快照测量数据](../server-rust/benchmarks/live-snapshot-2026-10-09.json)。
该微基准描述快照构造与析构的复制成本；SteamVR / VRChat 整体分位数通过完整输出场景测量。墙钟尾延迟包含系统抢占，本轮测试期间环境还有其他测试进程。
姿态 tick 创建一个共享 `Arc`，这部分成本属于 tick 阶段测量范围。
设备动态读数仍复制；外部追踪小表与 SteamVR 状态仍按值复制。

配置保存使用独立 OS 线程，保留原子的 YAML 备份、`sync_all()` 和替换逻辑。
慢磁盘测试用阻塞的写入函数验证：首个写入阻塞期间，生产方继续提交 99 次
配置并推进 PoseEngine；队列只有正在写的版本和最新待写版本，释放后仅
写入 revision 1 与 100，退出等待完成。额外测试验证保存失败的通知、失败后
保留内存配置，以及客户端回复等待持久化 revision。阻塞替身用于验证调度边界，实际磁盘耗时通过保存指标测量。启动保存、离线工具、BVH 与 replay
journal 仍使用各自原来的同步接口。
