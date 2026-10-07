# Rust SteamVR 桥接

Rust 已接入原版 OpenVR Driver 的 Protobuf 桥：头显和左右控制器位姿进入算法核心，计算追踪器输出给驱动。现有 GUI 的 SteamVR 共享开关、自动共享、连接检查和 YAML 保存已接通；Bindings Provider 的本地 SolarXR RPC 入口也已实现。

本次复用原版驱动与 Bindings Provider，没有替换驱动。使用 **协议版本 2 的 SlimeVR OpenVR Driver**；Tauri 已添加官方 v6.0.0 驱动资源、注册与启用流程，详见 [日常流程实施](rust-daily-workflow.zh-CN.md#steamvr-驱动管理)。当前环境没有真实 SteamVR/头显；自动测试使用模拟客户端和实际 Java 消息编码器，不能代替实机验收。

## 启动

```sh
pnpm rust:build
./server-rust/target/release/slimevr-server listen \
  --api-bind 127.0.0.1:21110 --config /path/to/vrconfig.yml
```

Windows 使用 `.exe`。Linux/Windows 的 API 模式默认启动 SteamVR 桥，`pnpm tauri:rust` 同样默认启用。然后启动 SteamVR，确认前端连接检查通过，批准和分配 tracker，执行 Full / Mounting reset。

| 平台 | 驱动位姿入口 | Bindings Provider RPC 入口 |
| --- | --- | --- |
| Windows | `\\.\pipe\SlimeVRDriver` | `\\.\pipe\SlimeVRRpc` |
| Linux | 原版 socket 目录中的 `SlimeVRDriver` | 同目录 `SlimeVRRpc` |

Linux 目录沿用原版顺序：`SLIMEVR_SOCKET_DIR` 优先；Steam Pressure Vessel 环境使用 `$XDG_DATA_HOME/dev.slimevr.SlimeVR` 或 `~/.local/share/dev.slimevr.SlimeVR`；普通环境使用 `$XDG_RUNTIME_DIR`，最后使用临时目录。驱动本身也有路径搜索规则，使用自定义目录时必须确保它能找到 socket。

可用参数，CLI 与 Tauri 均支持：

- `--no-steamvr`：关闭本次桥接；适合纯录制、回放或独立接收测试。
- `--steamvr-endpoint <路径>`：覆盖驱动管道/socket；Linux RPC 在同目录创建，Windows RPC 保持原版固定名称。该参数可在不启动 WebSocket API 时显式启用桥和配置加载。
- `--bindings-provider <可执行文件>`：指定原版 Bindings Provider。
- `--no-bindings-provider`：关闭辅助进程，位姿桥仍运行。

默认查找当前工作目录、后端可执行文件目录及 PATH 中的 `SlimeVR-Bindings-Provider.exe` / `slimevr-bindings-provider`。Tauri 也会查找其资源目录。收到协议版本 2 后延迟 3 秒启动辅助程序，断线和后端退出时回收；重连后可重新启动。辅助程序负责控制器按钮上的复位/暂停绑定。当前 Tauri 已打包官方 Driver；Bindings Provider 仍可复用已有安装或显式指定程序路径。

桥由后端监听，驱动主动连接。端点被其他服务占用时启动明确报错；Unix 只清理已失效的 socket，保留普通文件及正在使用的 socket。正常退出移除自己创建的 socket。可用 `--no-steamvr` 独立启动 GUI 后端进行排查。

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

标准角色与原版 `TrackerRole` 数字一致，串号使用 `human://WAIST`、`human://LEFT_FOOT` 等原版名称；重连保持稳定。输入和输出 tracker ID 属于独立命名空间，不把驱动的 OpenVR 设备编号当成 UDP sensor 身份。

## 输入、输出和生命周期

- 输入：头显 ID 0 或 HEAD/HMD 角色、左右控制器/手角色，位置单位为米，四元数 `{w,x,y,z}`，不重复应用 UDP 坐标转换。其他外部 tracker 类型当前只识别元数据，不作为身体输入接入。
- 位置必须完整且有限，旋转必须有限且非零。断开、错误状态、500 ms 没有新位姿以及桥断线会清除对应外部输入。旋转降级输入可以保留头部方向，但不把旧控制器位置或旧头显位置当作当前世界锚点。
- 输出：读取 `PoseSnapshot.skeleton.computed`，包含 LegTweaks 的最终位置/方向；腰使用计算 hip。输出由驱动消息驱动，核心仍按自身 tick 解算，慢 IPC 不阻塞解算循环。
- 有效世界锚点存在时发送有效状态与姿态；失去锚点时报告无效状态，避免把相对骨架作为有效世界坐标发送。关闭共享时发送 DISCONNECTED；重连重新注册当前角色。
- 电池：按共享角色聚合相应 Wi-Fi 设备的最低有效电量，保留原版按电压判断充电的规则；缺失值不伪造电量。入站外部设备电池、速度目前未作为独立 GUI/核心数据发布。派生速度输出已实现，开关为 `velocityConfig.sendDerivedVelocity`；默认关闭，详见 [算法说明](rust-derived-velocity.zh-CN.md)。
- 命令：Protobuf 的 Full/Yaw/Mounting、脚部 Mounting 和暂停命令可用；原 Bindings Provider 的 SolarXR 复位/暂停请求通过 `SlimeVRRpc` 进入同一服务。
- 桥的姿态和操作写入现有 journal 的控制记录，在线最终姿态与离线回放可比较。手动 WebSocket 位姿覆盖某个部位后，桥断开不会清除该手动来源；持续并发输入仍按最近处理的消息生效。

消息为小端 4 字节长度头，长度包含头部自身。Protobuf 帧上限 1024 字节，本地 SolarXR 帧上限 64 KiB；处理分包、连续多包、非法长度和截断。输入队列上限 128，输出批次队列上限 4；输出携带会话号，丢弃旧连接积压，队列满时保留未发送的注册变更以便重试。

`backend_info` 增加 `steamvr` 能力及连接、版本、端点、辅助进程和错误状态。连接检查使用原 SolarXR `STEAMVR_DISCONNECTED` 步骤；没有读取驱动安装列表时，不伪造“驱动已安装”或“安全模式禁用”等判断。

## 验证

协议固定为 SlimeVR/SlimeVR-OpenVR-Driver 提交 `dcc0f56bcb2a3196d6f92b1ed1d029faa425b931` 的 `src/bridge/ProtobufMessages.proto`，保留原 MIT 声明；Rust 由 prost-build 生成类型。描述符与本仓库提交 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6` 的 `ProtobufMessages.java` 一致，仅忽略编译器补充的 JSON 名称和源码定位。

9 项 Rust 桥接测试覆盖实际 Java 生成的 20 类消息逐字节编码、分包/非法帧、状态与超时、最终输出、背压、socket 保护与重连、辅助进程回收、真实 CLI/本地 RPC/共享配置，可选速度字段，以及在线发布帧与回放逐帧一致。前端新增协议联调覆盖 SteamVR 能力、原版消息输入、连接步骤、共享修改及 YAML 写回。

```sh
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
cargo clippy --manifest-path server-rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
pnpm --dir gui test:backend
pnpm --dir gui test:desktop
python3 server-rust/tools/generate-steamvr-golden.py
```

参考生成需要 Python 3、JDK、首次访问 Maven Central；仅有 JRE 时可通过 `--javac-java-modules <完整JDK的jmods目录>` 调用编译器。普通 Rust 测试使用已生成 fixture，不需要 Java。

当前验证包含 112 项 Rust、17 项 Node 前端 / 桌面、5 项 Tauri 测试，以及类型检查、Clippy、Linux release / Debian 包和 Windows GNU 后端编译。旧协议 1 SlimeVRInput feeder、Vive / Tundra 身体来源、VRChat 配置和原版 Bindings Provider 打包均已接通。真实 SteamVR、Windows 命名管道、OpenVR 按钮绑定和六点追踪仍需 [集中实测](rust-unified-hardware-test.zh-CN.md)。

代码入口：`server-rust/crates/slimevr-server/src/steamvr/`，原版 schema 和出处：`server-rust/crates/slimevr-server/proto/`；前后端范围见 [联调说明](rust-frontend-integration.zh-CN.md)。
