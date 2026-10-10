# Rust SteamVR 桥接

Rust 通过原版 OpenVR Driver 的 Protobuf 桥接收和输出姿态：头显和左右控制器位姿进入算法核心，计算追踪器输出给驱动。现有 GUI 的 SteamVR 共享开关、自动共享、连接检查和 YAML 保存已接通；Bindings Provider 的本地 SolarXR RPC 入口也已实现。

桥接使用 **协议版本 2 的 SlimeVR OpenVR Driver** 和原版 Bindings Provider，桌面包提供固定版本 v6.0.0 驱动资源、注册与启用流程，见下文的驱动注册与启用。自动参考使用模拟客户端与实际 Java 消息编码器，真实头显和追踪品质按实机清单验收。

## 启动

```sh
pnpm rust:build
./server-rust/target/release/slimevr-server listen \
  --api-bind 127.0.0.1:21110 --config /path/to/vrconfig.yml
```

Windows 使用 `.exe`。Linux/Windows 的 API 模式默认启动 SteamVR 桥，`pnpm tauri:rust` 同样默认启用。然后启动 SteamVR，确认前端连接检查通过，批准和分配 tracker，执行 Full / Mounting reset。

| 平台    | 驱动位姿入口                         | Bindings Provider RPC 入口 |
| ------- | ------------------------------------ | -------------------------- |
| Windows | `\\.\pipe\SlimeVRDriver`             | `\\.\pipe\SlimeVRRpc`      |
| Linux   | 原版 socket 目录中的 `SlimeVRDriver` | 同目录 `SlimeVRRpc`        |

Linux 目录沿用原版顺序：`SLIMEVR_SOCKET_DIR` 优先；Steam Pressure Vessel 环境使用 `$XDG_DATA_HOME/dev.slimevr.SlimeVR` 或 `~/.local/share/dev.slimevr.SlimeVR`；普通环境使用 `$XDG_RUNTIME_DIR`，最后使用临时目录。驱动本身也有路径搜索规则，使用自定义目录时必须确保它能找到 socket。

可用参数，CLI 与 Tauri 均支持：

- `--no-steamvr`：关闭本次桥接；适合纯录制、回放或独立接收测试。
- `--steamvr-endpoint <路径>`：覆盖驱动管道/socket；Linux RPC 在同目录创建，Windows RPC 保持原版固定名称。该参数可在不启动 WebSocket API 时显式启用桥和配置加载。
- `--bindings-provider <可执行文件>`：指定原版 Bindings Provider。
- `--no-bindings-provider`：关闭辅助进程，位姿桥仍运行。

默认查找当前工作目录、后端可执行文件目录及 PATH 中的 `SlimeVR-Bindings-Provider.exe` / `slimevr-bindings-provider`。Tauri 也会查找其资源目录。收到协议版本 2 后延迟 3 秒启动辅助程序，断线和后端退出时回收；重连后可重新启动。辅助程序负责控制器按钮上的复位/暂停绑定。当前 Tauri 已打包官方 Driver；Bindings Provider 仍可复用已有安装或显式指定程序路径。

桥由后端监听，驱动主动连接。端点被其他服务占用时启动明确报错；Unix 只清理已失效的 socket，保留普通文件及正在使用的 socket。正常退出移除自己创建的 socket。可用 `--no-steamvr` 独立启动 GUI 后端进行排查。

## 驱动注册与启用

Tauri 资源包含官方 SlimeVR OpenVR Driver **v6.0.0** 的 Windows x64、Linux x64／aarch64 发布包和许可证。`gui/scripts/fetch-steamvr-drivers.py` 固定发布 URL 与下载包 SHA-256；`pnpm tauri:dev`、`tauri:build`、`tauri:rust` 自动准备资源，已缓存时不重复下载。直接调用 Tauri CLI 前需先执行 `pnpm --dir gui tauri:drivers`。

后端读取 OpenVR 的 `openvrpaths.vrpath`，调用原版 `vrpathreg finddriver slimevr`／`adddriver <目录>`。已有注册保持原状；发现 SteamVR 的手动安装目录时保留安装并报告普通提示。Tauri / React 使用 Fluent 显示提示，同一次界面启动中显示一次；GPUI 在 SteamVR 设置页显示驱动状态。简体中文和英文使用对应翻译，其余语言沿用 Fluent 回退。注册工具执行异常时返回错误。Tauri 将打包资源中的平台驱动路径传给 Rust。驱动自动注册支持 Windows / Linux。

使用 SteamVR 本地 HTTP 服务的 `/drivers/list.json` 读取实际安装、启用和安全模式状态。原前端 EnableSteamVRDriverRequest 调用 `/drivers/unblock` 和 `/drivers/setenable`，带原版 Referer，验证启用结果后请求 `vrmonitor://restartsystem`。SteamVR 未运行时状态保持“未知”。

驱动状态进入原 `TrackingChecklistSteamVRDisconnected`；注册错误也保留在 backend_info，使稍后连接／重连的前端可看到。ServerInfos 的 localIp 返回显式绑定地址，未指定地址则选本机 IPv4 网卡；无可用 LAN 地址时回退到 loopback。

后端 CLI 参数：

| 参数                              | 用途                                                |
| --------------------------------- | --------------------------------------------------- |
| `--steamvr-driver <目录>`         | 明确指定带 driver.vrdrivermanifest 的驱动目录       |
| `--steamvr-runtime <目录>`        | 覆盖 SteamVR runtime 自动定位                       |
| `--no-driver-install`             | 保留桥接，跳过自动注册                              |
| `--no-steamvr-restart`            | 启用驱动后由使用者控制重启                          |
| `--steamvr-http <本地 HTTP 地址>` | 覆盖管理端点，默认 127.0.0.1:27062；仅允许 loopback |

## 共享设置

直接使用原版配置节点：

```yaml
bridges:
  steamvr:
    automaticSharedTrackersToggling: false
    trackers:
      waist: true
      chest: false
      left_foot: true
      right_foot: true
      left_knee: false
      right_knee: false
      left_elbow: false
      right_elbow: false
      left_hand: false
      right_hand: false
```

前端“设置 → 常规 → SteamVR”可修改这些字段。自动共享默认开启，按已分配的 tracker 部位决定胸、腰、膝、脚和肘的输出：腰在任意躯干 tracker 存在时启用；脚支持小腿或脚 tracker。自动模式保留左右手的手动选择，并在暂停期间保留共享状态。手追踪器会影响控制器跟踪，继续使用已有界面的确认提示。

标准角色与原版 `TrackerRole` 数字一致，串号使用 `human://WAIST`、`human://LEFT_FOOT` 等原版名称；重连保持稳定。输入与输出 tracker ID、OpenVR 设备编号和 UDP sensor 身份分别映射。

## 输入、输出和生命周期

- 输入：头显 ID 0 或 HEAD/HMD 角色、左右控制器/手角色，位置单位为米，四元数 `{w,x,y,z}`，按 SteamVR 坐标直接适配。Vive / Tundra 等外部身体 tracker 按身份与配置绑定接入。
- 位置必须完整且有限，旋转必须有限且非零。断开、错误状态、500 ms 没有新位姿以及桥断线会清除对应外部输入。旋转降级输入可以保留头部方向，当前世界锚点要求有效的位置来源。
- 输出：读取 `PoseSnapshot.skeleton.computed`，包含 LegTweaks 的最终位置/方向；腰使用计算 hip。输出由驱动消息驱动，核心仍按自身 tick 解算，主循环使用非阻塞队列与 IPC 任务交换输出。
- 有效世界锚点存在时发送有效状态与姿态；失去锚点时报告无效状态，世界输出有效性按当前锚点状态判断。关闭共享时发送 DISCONNECTED；重连重新注册当前角色。
- 电池：按共享角色聚合相应 Wi-Fi 设备的最低有效电量，保留原版按电压判断充电的规则；缺失值标记为未知。入站设备元数据与姿态按当前来源适配层发布，派生速度由核心位置计算。派生速度输出已实现，开关为 `velocityConfig.sendDerivedVelocity`；默认关闭，见下文的派生速度。
- 命令：Protobuf 的 Full/Yaw/Mounting、脚部 Mounting 和暂停命令可用；原 Bindings Provider 的 SolarXR 复位/暂停请求通过 `SlimeVRRpc` 进入同一服务。
- 桥的姿态和操作写入现有 journal 的控制记录，在线最终姿态与离线回放可比较。手动 WebSocket 位姿覆盖某个部位后，该部位由手动来源管理清理；持续并发输入仍按最近处理的消息生效。

消息为小端 4 字节长度头，长度包含头部自身。Protobuf 帧上限 1024 字节，本地 SolarXR 帧上限 64 KiB；处理分包、连续多包、非法长度和截断。输入队列上限 128，输出批次队列上限 4；输出携带会话号，丢弃旧连接积压，队列满时保留未发送的注册变更以便重试。

`backend_info` 提供 `steamvr` 能力及连接、版本、端点、辅助进程和错误状态。连接检查使用原 SolarXR `STEAMVR_DISCONNECTED` 步骤；驱动安装、启用和安全模式状态使用管理接口读取结果，读取失败时保持未知。

## 手部来源切换

后端按身体部位记录最后发布有效姿态的设备 ID。清理消息按这个所有者 ID 判断是否清除当前节点。新设备接管后，节点使用新设备的姿态；每个设备的失效消息清理自身缓存。

当前所有者断开、超时或失去定位时清除对应输入，恢复状态与有效姿态后重新接入。SolarXR 设备列表按当前有效来源生成。

自动回归覆盖两个切换方向、旧来源的断开 / 超时 / 重新注册 / 无定位消息、当前来源清理与恢复，并核对实际发送给两个前端的设备列表。实际头显、切换入口和 SteamVR 版本按实机配置验收。

手柄 → 手部追踪 → 手柄的切换和来源断开 / 恢复按 [实机清单](rust-unified-hardware-test.zh-CN.md#steamvr) 验收，记录头显型号、切换入口、节点状态与时间。

## 派生速度

前端“设置 → 常规 → 速度设置”可开启“发送派生速度”；直接编辑原版配置也可以：

```yaml
velocityConfig:
  sendDerivedVelocity: true
```

缺省为 `false`，沿用原版选择。启用后，每个解算 tick 独立计算 11 个计算追踪器的速度；SteamVR 输出的 Position 消息按需填入 `vx`、`vy`、`vz`。关闭时省略三个字段，驱动按原协议处理为没有速度输入。前端能力通知增加 `derived_velocity`，设置读取、修改、恢复默认值及保存沿用已有 SolarXR 消息。

`PoseConfig.send_derived_velocity` 为核心开关。`PoseSnapshot.computed_velocities` 按计算追踪器名称返回有效速度；没有有效数据时省略该 JSON 字段。journal 使用原有配置、控制和 tick 记录，可以重建相同速度。

### 计算与时序

`v = (当前位置 - 上次位置) / dt`。首次观察只建立基线；有效间隔为 **100 微秒至 250 毫秒，包含端点**。零间隔、倒退时钟或过长/过短间隔重新建立位置基线，该次速度为缺失。除法使用双精度秒数，再转换为 f32，与原版一致。

独立 `DerivedVelocity` 使用显式微秒时钟；现有 PoseEngine / journal 使用毫秒时钟，所以解算循环可表示的有效间隔是 1–250 ms。速度计算频率取决于解算 tick，与 UDP 样本频率及驱动消息发送频率分开。

集成有以下明确选择：

- 原版 HumanSkeleton 在更新 FK 计算追踪器时计算速度，之后才执行 LegTweaks。Rust 从 LegTweaks 后的最终 `skeleton.computed` 计算速度，使输出速度与发送给驱动的位置一致；两种实现分别按其阶段顺序测量输出。
- 仅在有外部世界位置锚点且未暂停时计算；SteamVR 世界速度使用外部锚点下的最终位置。丢失锚点或暂停会清除历史，恢复后的首帧只建立基线。
- Full/Yaw/Mounting reset、外部来源移除、设备会话重启、绑定/安装修正变更、骨长或滤波/腿部/对齐/定位参数变化会清除历史，使下一帧从当前有效位置重建基线。临时腿部参数和测眼高后的骨长更新同样处理。
- 非有限位置或溢出结果标记为缺失。计算结果直接作为线速度输出，有效零速度发送为零。

该速度由输出位置差分产生，输入为最终位置与显式时间。

### 速度验证

参考生成器提取仓库真实 Kotlin `Tracker.updateDerivedVelocity()` 方法，仅将单调时钟替换为 `TestTimeSource`。两个参考序列共 18 次观察覆盖 99/100 微秒、250000/250001 微秒、零 dt、关闭/重新开启、位置丢失和历史重置；Rust 输出按现有 `3e-6 × (1 + abs(expected))` 分量容差比较，数据缺失必须一致。

核心测试覆盖最终脚部位置、平移、无世界锚点、复位/暂停/重连、配置变更、异常位置和数值溢出。SteamVR 测试验证可选速度字段及失效行为；真实 CLI + socket + RPC 测试启用速度，并逐个比较在线发布帧和 journal 回放帧。配置测试覆盖缺省、字段校验、保存/重载和未知字段保留；前端绑定测试覆盖能力、读取和关闭后的 YAML 保存。

```sh
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
python3 server-rust/tools/generate-core-golden.py
pnpm --dir gui test:backend
```

两个 Kotlin 速度参考序列与核心 / 输出 / 配置检查覆盖上述窗口与状态边界。真实 SteamVR 和六点追踪器的速度表现按实机记录验收。

实现：[velocity.rs](../server-rust/crates/slimevr-core/src/velocity.rs)、[pose.rs](../server-rust/crates/slimevr-core/src/pose.rs)、[SteamVR 桥](../server-rust/crates/slimevr-server/src/steamvr/mod.rs)。

## 桥接验证

协议固定为 SlimeVR/SlimeVR-OpenVR-Driver 提交 `dcc0f56bcb2a3196d6f92b1ed1d029faa425b931` 的 `src/bridge/ProtobufMessages.proto`；Rust 由 prost-build 生成类型。描述符与本仓库提交 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6` 的 `ProtobufMessages.java` 一致，仅忽略编译器补充的 JSON 名称和源码定位。

Rust 桥接测试覆盖实际 Java 生成的 20 类消息逐字节编码、分包/非法帧、状态与超时、最终输出、背压、socket 保护与重连、辅助进程回收、真实 CLI/本地 RPC/共享配置，可选速度字段，以及在线发布帧与回放逐帧一致。前端协议联调覆盖 SteamVR 能力、原版消息输入、连接步骤、共享修改及 YAML 写回。

```sh
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
cargo clippy --manifest-path server-rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
pnpm --dir gui test:backend
pnpm --dir gui test:desktop
python3 server-rust/tools/generate-steamvr-golden.py
```

参考生成需要 Python 3、JDK、首次访问 Maven Central；仅有 JRE 时可通过 `--javac-java-modules <完整JDK的jmods目录>` 调用编译器。普通 Rust 测试读取已提交的 fixture。

CI 按平台执行后端、前端通信和宿主检查，具体结果以该提交的 Actions 记录为准。旧协议 1 SlimeVRInput feeder、Vive / Tundra 身体来源、VRChat 配置和原版 Bindings Provider 打包均已接通。真实 SteamVR、Windows 命名管道、OpenVR 按钮绑定和六点追踪仍需 [集中实测](rust-unified-hardware-test.zh-CN.md)。

代码入口：`server-rust/crates/slimevr-server/src/steamvr/`，原版 schema 和出处：`server-rust/crates/slimevr-server/proto/`；前后端范围见 [联调说明](rust-frontend-integration.zh-CN.md)。
