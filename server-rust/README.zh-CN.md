# SlimeVR Rust 接收端与算法核心

后端提供独立 UDP 接收模式：现有固件握手、设备与 sensor 注册、方向/加速度接收、遥测、心跳、超时与重连，以及离线录制回放。

算法核心已接通实时接收和回放：全身骨架/约束、脚部修正、对齐/定位、flex 与校准，以及离线 AutoBone。运行方式、99 组核心参考与 14 组 AutoBone 训练对照与当前范围见 [算法核心说明](README.core.zh-CN.md)。

源码基线是 SlimeVR-Server `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。接收层使用已有 fused quaternion 协议，适配资料中的 `protocol=22 / firmware=good / IMU=13` 字段。固件版本（包括 `good`）按握手字符串保存。

CLI 提供接收、实时解算和回放命令。原 YAML 迁移、SteamVR / HID / OSC 输入输出、串口 / 固件、SolarXR API 与 Tauri 后端集成已接入，见 [当前交接说明](../docs/rust-completion-worklog.zh-CN.md)。

## 启动

安装 Rust 1.88 或更新版本，在仓库中运行：

```sh
cd server-rust
cargo run --locked -p slimevr-server -- listen --accept-new-devices
```

默认监听 `0.0.0.0:6969`，在没有在线 sensor 时每十秒向本机 IPv4 网段广播发现包。默认 `info` 记录连接与状态变化，保留警告和错误；加 `--log-level debug` 可查看每秒的完整设备状态，`trace` 可查看逐样本数据。先停止占用同一个 UDP 端口的 Java 服务。

设备准入有两个选择：`--accept-new-devices` 接受所有成功握手的设备；或重复传入 `--allow-device` 指定 MAC。没有指定准入参数时，未知设备进入待批准流程并产生 `device_pending` 诊断；批准后通过重试握手注册。

```sh
cargo run --locked -p slimevr-server -- listen \
  --allow-device AA:BB:CC:DD:EE:01 \
  --allow-device AA:BB:CC:DD:EE:02
```

MAC 用于协议身份与准入匹配。API 或显式配置模式从 YAML 的 `knownDevices` 读取批准名单；纯接收模式使用 CLI 准入参数与内存注册表。

其他参数：

| 参数                                    | 用途                                                                                                            |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `--bind 127.0.0.1:16969`                | 指定测试地址/端口                                                                                               |
| `--no-discovery`                        | 关闭广播，适合本地模拟测试                                                                                      |
| `--discovery-target 192.168.1.255:6969` | 指定发现目的地址，可重复；替代自动网段检测                                                                      |
| `--pose-config pose.json`               | 启用算法核心，配置身体绑定与算法参数                                                                            |
| `--pose-ms 4`                           | 实时解算周期，单位 ms                                                                                           |
| `--pose-output-ms 20`                   | debug / trace 下的姿态 JSON 输出周期，独立于解算周期                                                            |
| `--log-level info`                      | error / warn / info / debug / trace；也支持 `SLIMEVR_LOG_LEVEL`，详见 [日志说明](../docs/rust-logging.zh-CN.md) |
| `--events`                              | 没有指定级别或环境变量时等效于 trace，数据量较大                                                                |
| `--summary-ms 1000`                     | debug / trace 下的完整设备状态输出间隔；也控制 journal 刷新                                                     |
| `--run-for 60`                          | 运行指定秒数后正常结束                                                                                          |
| `--record recordings/run.jsonl`         | 以独占创建方式打开新的录制文件                                                                                  |

Windows PowerShell 可使用单行命令；正常退出用 Ctrl+C，或使用 `--run-for`。

## 录制和回放

先创建 `recordings` 目录，然后录制，例如：

```sh
cargo run --locked -p slimevr-server -- listen --accept-new-devices --run-for 60 --record recordings/run.jsonl
cargo run --locked -p slimevr-server -- replay recordings/run.jsonl
```

回放在本地按记录的接收次序和单调时钟重新执行同一个接收器，逐条核对其生成的回复地址和字节，最后输出状态及 `replay_complete`。

JSONL 版本 1 包含 `header`（接收配置）、`receive`（来源、时间、原始字节 hex）、`tick`、`send`（发送意图）和正常结束的 `end`。journal 记录接收器状态相关事件；发现广播由运行时管理。`send` 表示回复意图，实际交付需结合设备接收确认判断。异常退出造成的缺少 `end`、截断记录、时钟倒退或回复差异都会导致回放失败。在 `debug` / `trace` 下输出的最终 `snapshot`，移除诊断字段 `level` 后应与回放相同。完整快照在 `debug` / `trace` 下输出，journal 按记录格式保存回放所需的数据。

录制保留原始 UDP 字节，因此也可能包含设备的 Serial 内容；Serial 文本使用相应详细诊断级别输出。Unix 创建文件权限为 `0600`。录制文件在指定路径连续保存，分享前需处理设备与串口信息。Tauri 桌面诊断日志另有轮转，见 [日志说明](../docs/rust-logging.zh-CN.md)。

也可以直接解析一个包：

```sh
cargo run --locked -p slimevr-server -- decode 000000000000000000000001
```

## 数据与兼容约定

接收状态由单个任务持有，协议解析和状态机可以不依赖网络单独运行。`slimevr-core` 定义四元数、向量、Timed 数据、`TrackerSample` 与 `InputEvent`，PoseEngine 使用这些输入执行算法并发布共享姿态快照。

| 协议内容                          | 当前处理                                                                           |
| --------------------------------- | ---------------------------------------------------------------------------------- |
| 握手 `3`                          | 保存 board/IMU/MCU/协议/固件/MAC；返回特殊 64 字节握手                             |
| SensorInfo `15`                   | 按 sensor ID 注册，支持多个 sensor；返回不带序号头的 6 字节 ACK                    |
| 方向 `1/16/17`                    | 读取浮点 `x,y,z,w`；`17` 的 normal 类型生成方向样本，correction 类型仅诊断         |
| 方向+加速度 `23`                  | Q15 方向除以 32768 后单位化，Q7 加速度除以 128                                     |
| 加速度 `4`                        | 支持可选 sensor ID，缺省为 0                                                       |
| bundle `100/101`                  | 支持普通及紧凑子包头、零长度条目；未知子包可跳过，不递归解包                       |
| FeatureFlags `22`                 | 回复支持普通/紧凑 bundle 的能力位 `03`，保留固件能力字节                           |
| 心跳/keepalive/ping               | 发送 `1` 的 keepalive 和 `10` 的 ping；匹配 ping 计算半 RTT                        |
| 电池 `12`、RSSI `19`、温度 `20`   | 保存实际值及各自更新时间；接收器保留 wire fraction，前端按电量显示规则转换为百分比 |
| error/tap/action `14/13/21`       | 记录错误码和状态、敲击和用户动作；算法核心处理复位与暂停动作                       |
| flex/position `26/27`             | 保存接收值，position 不刷新方向新鲜度                                              |
| Serial/config ACK/protocol change | 原始录制与诊断；设备磁力计 SET_CONFIG / ACK 已接通，包 200 沿用原版忽略协商的行为  |

`TrackerSample` 同时保留 packet 与 server 坐标值。方向仅执行 UDP 固有的 `q_server = q_axes × q_packet`，其中 `q_axes` 是绕 X 轴 `−π/2`。加速度在协议 `<22` 时转换为 `(y,−x,z)`，协议 `≥22` 时直接使用。板端 `IMU_ROTATION` 由固件应用，佩戴校准由核心校准层应用。

`received_at_ms` 使用本次运行的单调处理时刻，驱动状态机与 replay；可选 `socket_received_at_ms` 保存 UDP socket 接收时刻，用于姿态年龄与排队延迟。设备采样时刻字段 `sensor_timestamp_us` 在当前协议中为 `null`。缺失温度、电压等为 `null`，已接收值保留各自更新时间。诊断字段按协议实际提供的数据产生。

`transport_timed_out` 判断设备是否仍在发包，`SensorStatus` 沿用 2 秒 timeout、5 秒 disconnected 的心跳语义。`freshness.pose_age_ms/pose_stale` 单独描述方向停更；从未收到方向时为 `null`。方向样本计数随实际方向输入增加。收到方向或加速度可恢复超时状态，设备报告的 Error 由 SensorInfo 状态更新解除。

序号按 Java 的 signed 64-bit 比较，普通包在 `sequence == 0` 或大于上一序号时接受，0 也会复位比较基线。计数分别列出 received、accepted、malformed、unknown_sources、admission_denied、sequence_rejected、sequence_gaps、ignored_packets、samples；gap 表示已处理序号的缺口，原因需结合队列合并和网络诊断判断。计数为本次进程累计值。

接收器执行以下校验与会话规则：

- 合法的独立握手允许重置会话，同 MAC 换地址时保持 identity、增加 session；清空旧 sensor 数据，等待当前固件重新发送 SensorInfo。新会话接受当前地址与会话的样本。已知 UDP 设备重新握手时保留算法校准，并重新建立动态姿态与滤波历史。
- bundle 先全部解析，任何已支持子包被截断即整包拒绝。设备存活时间与序号由通过校验的包更新。
- 浮点方向全零/含 NaN 时采用 Java 的 identity fallback；NaN 标量替换为 0，同时产生诊断。无限值、转换溢出及全零紧凑方向拒绝。有效紧凑方向要求至少一个非零分量。
- 固件长度按 unsigned byte 读取。29 个协议参考样本覆盖短版本字符串，较长字段按 Rust 解析器的长度校验处理。
- 原始数字枚举与固件能力字节保留，用于第三方设备诊断；硬件与分配操作按已识别的能力执行。旧固件的缺省 sensor 0 已实现，完整 owoTrack 行为仍需独立验收。

设备表默认最多 64 个设备，每个设备最多 256 个 sensor。待批准来源使用有界诊断，CLI 的 pending 提示每秒最多一条。准入和身体绑定由原 YAML 持久化；实时 sensor / 会话注册表使用内存状态，重启后由设备重新注册。离线设备按已知身份保留，删除操作显式执行。

## 验证

```sh
cargo fmt --package slimevr-core --package slimevr-server -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

接收测试覆盖协议、状态、录制、Kotlin 参考以及实际 UDP socket 与 CLI 回放。后者使用六个**模拟设备**，覆盖双 sensor、重复/乱序、紧凑包、截断包，并比较录制与回放的完整最终状态；同时核对在线与回放最终姿态一致，以及重复解算逐字节一致。核心测试见 [算法说明](README.core.zh-CN.md)。

`tests/fixtures/udp-golden.json` 包含 29 个合成输入的参考结果及三个回复字节。参考结果通过实际编译运行仓库中的 `UDPProtocolParser.kt`、`UDPPacket.kt` 和 `ktmath` 得到；仅 TrackerPosition lookup 与未使用的 UDPDevice 外部依赖用 stub 隔离。该参考的执行范围为解析与坐标数学。fixture 包含 commit 和相关源码 SHA-256，普通 `cargo test` 读取已提交的 fixture。

重新生成参考样本需要 Java 17+、Python 3 和首次访问 Maven Central：

```sh
python3 tools/generate-udp-golden.py
```

CI 在 Linux / Windows 执行后端检查。实际固件 bundle 组合、持续采样率、断网 / 重启与长时间无线表现按真实设备验收，见 [统一清单](../docs/rust-unified-hardware-test.zh-CN.md)。

## 下一步实机验收

1. 先连接一台保留原固件的设备，录制 60 秒，确认 `firmware=good`、`protocol=22`、实际 MAC 和 sensor 数量。
2. 观察 `packet_rotation/server_rotation`、加速度和姿态 age，确认单个 tracker 转动会改变对应 sensor 的方向；记录实际包量与更新频率。
3. 断网再恢复、重启设备，核对 timeout、session 增加、sensor 重注册、低序号恢复；离线回放同一文件。
4. 扩展到胸/髋/双大腿/双小腿六台设备，完成至少 30 分钟录制与内存、吞吐、拒绝原因检查。

算法层已经接通实时解算与回放，见 [算法核心说明](README.core.zh-CN.md)。HID、SteamVR、OSC / VMC 和前端通信由各自适配层接入；验收按来源与平台分别记录。
