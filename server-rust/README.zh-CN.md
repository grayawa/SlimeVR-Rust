# SlimeVR Rust 接收端与算法核心

目前可以独立运行 UDP 接收程序：现有固件握手、设备与 sensor 注册、方向/加速度接收、遥测、心跳、超时与重连，以及离线录制回放。

算法核心已接通实时接收和回放：全身骨架/约束、脚部修正、对齐/定位、flex 与校准，以及离线 AutoBone。运行方式、95 组核心参考与 14 组 AutoBone 训练对照与当前范围见 [算法核心说明](README.core.zh-CN.md)。

源码基线是 SlimeVR-Server `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。本批按已有 fused quaternion 协议实现，适配资料中的 `protocol=22 / firmware=good / IMU=13` 字段。`good` 保留为字符串，不要求 SemVer。

接收模式保留原有 CLI 约定，新增实时解算参数。原 YAML 迁移、SteamVR / HID / OSC 输入输出、串口 / 固件、SolarXR API 与 Tauri 后端集成已接入，见 [当前交接说明](../docs/rust-completion-worklog.zh-CN.md)。

## 启动

安装 Rust 1.88 或更新版本，在仓库中运行：

```sh
cd server-rust
cargo run --locked -p slimevr-server -- listen --accept-new-devices
```

默认监听 `0.0.0.0:6969`，在没有在线 sensor 时每十秒向本机 IPv4 网段广播发现包。默认 `info` 记录连接与状态变化，保留警告和错误；加 `--log-level debug` 可查看每秒的完整设备状态，`trace` 可查看逐样本数据。先停止占用同一个 UDP 端口的 Java 服务。

设备准入有两个选择：`--accept-new-devices` 接受所有成功握手的设备；或重复传入 `--allow-device` 指定 MAC。没有指定准入参数时，未知设备只产生 `device_pending` 诊断，不注册、不回复握手。

```sh
cargo run --locked -p slimevr-server -- listen \
  --allow-device AA:BB:CC:DD:EE:01 \
  --allow-device AA:BB:CC:DD:EE:02
```

MAC 只是协议中的设备标识，未提供密码学认证。程序当前使用独立内存注册表，不读取或写入 Java 的设备批准名单。

其他参数：

| 参数 | 用途 |
| --- | --- |
| `--bind 127.0.0.1:16969` | 指定测试地址/端口 |
| `--no-discovery` | 关闭广播，适合本地模拟测试 |
| `--discovery-target 192.168.1.255:6969` | 指定发现目的地址，可重复；替代自动网段检测 |
| `--pose-config pose.json` | 启用算法核心，配置身体绑定与算法参数 |
| `--pose-ms 4` | 实时解算周期，单位 ms |
| `--pose-output-ms 20` | debug / trace 下的姿态 JSON 输出周期，独立于解算周期 |
| `--log-level info` | error / warn / info / debug / trace；也支持 `SLIMEVR_LOG_LEVEL`，详见 [日志说明](../docs/rust-logging.zh-CN.md) |
| `--events` | 没有指定级别或环境变量时等效于 trace，数据量较大 |
| `--summary-ms 1000` | debug / trace 下的完整设备状态输出间隔；也控制 journal 刷新 |
| `--run-for 60` | 运行指定秒数后正常结束 |
| `--record recordings/run.jsonl` | 创建一个新的录制文件，不覆盖已有文件 |

Windows PowerShell 可使用单行命令；正常退出用 Ctrl+C，或使用 `--run-for`。

## 录制和回放

先创建 `recordings` 目录，然后录制，例如：

```sh
cargo run --locked -p slimevr-server -- listen --accept-new-devices --run-for 60 --record recordings/run.jsonl
cargo run --locked -p slimevr-server -- replay recordings/run.jsonl
```

回放不打开 socket、不等待真实时间。它按记录的接收次序和单调时钟重新执行同一个接收器，逐条核对其生成的回复地址和字节，最后输出状态及 `replay_complete`。

JSONL 版本 1 包含 `header`（接收配置）、`receive`（来源、时间、原始字节 hex）、`tick`、`send`（发送意图）和正常结束的 `end`。发现广播不进入 journal，因为它不改变接收器状态；UDP 发送成功不代表设备收到，`send` 不作为交付确认。异常退出造成的缺少 `end`、截断记录、时钟倒退或回复差异都会导致回放失败。在 `debug` / `trace` 下输出的最终 `snapshot`，移除诊断字段 `level` 后应与回放相同。默认 `info` 不输出快照，但录制与回放数据仍完整。

录制保留原始 UDP 字节，因此也可能包含设备的 Serial 内容；默认终端不输出 Serial 文本。Unix 创建文件权限为 `0600`。录制文件不加入 Git，录制 journal 不轮转。Tauri 桌面诊断日志另有轮转，见 [日志说明](../docs/rust-logging.zh-CN.md)。

也可以直接解析一个包：

```sh
cargo run --locked -p slimevr-server -- decode 000000000000000000000001
```

## 数据与兼容约定

接收状态由单个任务持有，协议解析和状态机可以不依赖网络单独运行。`slimevr-core` 目前只定义四元数、向量、带接收时间的数据、`TrackerSample` 与 `InputEvent`；下一阶段算法直接消费这些输入。

| 协议内容 | 当前处理 |
| --- | --- |
| 握手 `3` | 保存 board/IMU/MCU/协议/固件/MAC；返回特殊 64 字节握手 |
| SensorInfo `15` | 按 sensor ID 注册，支持多个 sensor；返回不带序号头的 6 字节 ACK |
| 方向 `1/16/17` | 读取浮点 `x,y,z,w`；`17` 的 normal 类型生成方向样本，correction 类型仅诊断 |
| 方向+加速度 `23` | Q15 方向除以 32768 后单位化，Q7 加速度除以 128 |
| 加速度 `4` | 支持可选 sensor ID，缺省为 0 |
| bundle `100/101` | 支持普通及紧凑子包头、零长度条目；未知子包可跳过，不递归解包 |
| FeatureFlags `22` | 回复支持普通/紧凑 bundle 的能力位 `03`，保留固件能力字节 |
| 心跳/keepalive/ping | 发送 `1` 的 keepalive 和 `10` 的 ping；匹配 ping 计算半 RTT |
| 电池 `12`、RSSI `19`、温度 `20` | 保存实际值及各自更新时间；电量保留 wire fraction，未加入 UI 百分比解释 |
| error/tap/action `14/13/21` | 记录错误码和状态、敲击和用户动作；复位动作在算法阶段执行 |
| flex/position `26/27` | 保存接收值，position 不刷新方向新鲜度 |
| Serial/config ACK/protocol change | 原始录制与诊断；设备磁力计 SET_CONFIG / ACK 已接通，包 200 沿用原版忽略协商的行为 |

`TrackerSample` 同时保留 packet 与 server 坐标值。方向仅执行 UDP 固有的 `q_server = q_axes × q_packet`，其中 `q_axes` 是绕 X 轴 `−π/2`。加速度在协议 `<22` 时转换为 `(y,−x,z)`，协议 `≥22` 时直接使用。这里不再应用板端已经做过的 `IMU_ROTATION`，也不实现佩戴校准。

`received_at_ms` 是本次运行的单调接收时间。现有协议没有设备采样时刻，`sensor_timestamp_us` 为 `null`。缺失的温度、电压等也为 `null`；已接收值带自己的时间戳，不因收到心跳而变成“刚更新”。不推导不存在的 raw gyro、FIFO overflow 或 IMU 丢样数据。

`transport_timed_out` 判断设备是否仍在发包，`SensorStatus` 沿用 2 秒 timeout、5 秒 disconnected 的心跳语义。`freshness.pose_age_ms/pose_stale` 单独描述方向停更；从未收到方向时为 `null`。ping/心跳不会增加方向样本数。收到方向或加速度可恢复超时状态，设备报告的 Error 由 SensorInfo 状态更新解除。

序号按 Java 的 signed 64-bit 比较，普通包在 `sequence == 0` 或大于上一序号时接受，0 也会复位比较基线。计数分别列出 received、accepted、malformed、unknown_sources、admission_denied、sequence_rejected、sequence_gaps、ignored_packets、samples；gap 仅表示序号缺口，不能直接证明 UDP 丢包或 IMU 丢样。当前计数为本次进程的累计值，不复刻 Java 每 10 秒清零的窗口。

以下是明确的兼容差异：

- 合法的独立握手允许重置会话，修正设备重启后低序号可能被旧会话拒绝的问题。同 MAC 换地址时保持 identity、增加 session；清空旧 sensor 数据，等待当前固件重新发送 SensorInfo。旧地址之后的包不会更新新会话。
- bundle 先全部解析，任何已支持子包被截断即整包拒绝，不像 Java 对长度越界先截取剩余字节。失败包不刷新存活时间或序号。
- 浮点方向全零/含 NaN 时采用 Java 的 identity fallback；NaN 标量替换为 0，同时产生诊断。无限值、转换溢出及全零紧凑方向拒绝。Java 的全零紧凑方向结果为 NULL；本接收器避免将其作为有效姿态。
- 固件长度按 unsigned byte 读取；Java 当前按 signed byte。29 个正常样本覆盖目前的短版本字段，长于 127 字节的版本字符串未做 Java 等价保证。
- 原始数字枚举与固件能力字节保留，用于第三方设备诊断；未知枚举尚未执行身体分配、硬件配置等行为。旧固件的缺省 sensor 0 已实现，完整 owoTrack 行为仍需独立验收。

设备表默认最多 64 个设备，每个设备最多 256 个 sensor。待批准来源不建立无限增长的缓存；CLI 的 pending 提示每秒最多一条。准入和身体绑定由原 YAML 持久化；实时 sensor / 会话注册表不落盘，重启后等待设备重新注册。离线历史设备不自动删除。

## 验证

```sh
cargo fmt --package slimevr-core --package slimevr-server -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

接收端保留 17 项集成测试：15 项协议/状态/录制测试，1 项 Kotlin 对照测试，1 项真实 UDP socket 与 CLI 回放测试。后者使用六个**模拟设备**，覆盖双 sensor、重复/乱序、紧凑包、截断包，并比较录制与回放的完整最终状态；现已扩展为实时解算与回放最终姿态完全一致，以及重复解算逐字节一致。新增核心测试见 [算法说明](README.core.zh-CN.md)。

`tests/fixtures/udp-golden.json` 包含 29 个合成输入的参考结果及三个回复字节。参考结果通过实际编译运行仓库中的 `UDPProtocolParser.kt`、`UDPPacket.kt` 和 `ktmath` 得到；仅 TrackerPosition lookup 与未使用的 UDPDevice 外部依赖用 stub 隔离。它验证解析与坐标数学，不代表运行过整个 Java 服务或接入过真实设备。fixture 包含 commit 和相关源码 SHA-256，普通 `cargo test` 不需要 JVM。

重新生成参考样本需要 Java 17+、Python 3 和首次访问 Maven Central：

```sh
python3 tools/generate-udp-golden.py
```

已在 Linux 执行测试与 Clippy；Windows GNU 交叉编译通过，尚未在 Windows 实际运行。当前环境没有用户的真实 tracker，实机连接、固件的实际 bundle 组合、六点持续采样率、断网/重启联调及 30 分钟压测仍待验证。

## 下一步实机验收

1. 先连接一台保留原固件的设备，录制 60 秒，确认 `firmware=good`、`protocol=22`、实际 MAC 和 sensor 数量。
2. 观察 `packet_rotation/server_rotation`、加速度和姿态 age，确认单个 tracker 转动会改变对应 sensor 的方向；记录实际包量与更新频率。
3. 断网再恢复、重启设备，核对 timeout、session 增加、sensor 重注册、低序号恢复；离线回放同一文件。
4. 扩展到胸/髋/双大腿/双小腿六台设备，完成至少 30 分钟录制与内存、吞吐、拒绝原因检查。

算法层已经接通实时解算与回放，见 [算法核心说明](README.core.zh-CN.md)。实机接收验收与其他接收来源仍需继续；前端通信进入下一阶段。
