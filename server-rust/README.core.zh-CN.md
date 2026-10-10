# Rust 算法核心使用说明

算法核心可独立运行，支持 `UDP → 校准/滤波 → 全身骨架与约束 → 脚部修正/对齐/定位 → PoseSnapshot`，并提供离线 AutoBone。实现位于独立的 `slimevr-core` 库，接收统一输入与显式时钟。`slimevr-server` 负责实时接收、场景执行与录制回放。

前端 API 已按配置、传输连接、应用状态和六类 RPC 拆分，当前入口与调用边界见 [后端 API 架构说明](../docs/rust-backend-api-architecture.zh-CN.md)。

参考版本为 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。算法接口通过受控输入与参考数据验证，实际追踪和 VR 输出按硬件场景验收。GPUI / Web / Tauri 前端共用服务端通信层，启动与功能边界见 [前后端联调说明](../docs/rust-frontend-integration.zh-CN.md)。

## 运行

在 `server-rust` 中执行，需要 Rust 1.88+。

### 六点场景

```sh
cargo run --locked -p slimevr-server -- solve examples/six-point.scene.jsonl --config examples/six-point.json --frames
```

场景包含 22 个解算帧、Full/Yaw reset、暂停及头显移动。布局为胸、髋、双大腿、双小腿。MAC 与运动数据均为合成示例，使用前将配置中的身份替换为实际设备。

位置单位为米，坐标为 `+X` 右、`+Y` 上、`+Z` 后；四元数字段顺序为 `{w,x,y,z}`。输入方向是全局方向，子骨位置继承父骨尾端，方向使用该骨的全局方向。

### 实时 UDP 解算与录制

```sh
cargo run --release --locked -p slimevr-server -- listen \
  --bind 0.0.0.0:6969 --accept-new-devices \
  --pose-config examples/six-point.json \
  --pose-ms 4 --pose-output-ms 20 --log-level debug --record recordings/run.jsonl
```

需先创建 `recordings` 目录。`--accept-new-devices` 显式开放新设备准入；也可使用接收端的 MAC 白名单。带 `--pose-config` 时，默认每 4 ms 解算一次；指定 `debug` / `trace` 才按 `--pose-output-ms` 周期打印完整姿态，默认 `info` 保留常规状态与警告。stdout 同时包含接收诊断，按 `type` 区分，warn / error 输出到 stderr。详见 [日志说明](../docs/rust-logging.zh-CN.md)。没有姿态配置、未传入 `--config` 且未启用 `--api-bind` 时仍是接收模式。启用 API 时直接使用原版 YAML，详见 [配置复用说明](../docs/rust-config-compatibility.zh-CN.md)。

解算频率与 JSON 输出频率分离，journal 保存每个实际 tick。主机 tick 与设备采样率分别测量。普通 UDP 没有 HMD 世界位置，输出为相对骨架；启用 `localizer` 可估计根位置，其结果为相对根位置估计。HMD/控制器可通过库 API 或 scene 注入，SteamVR 驱动协议 2 桥已接入，见 [桥接说明](../docs/rust-steamvr-bridge.zh-CN.md)。

### 回放解算

```sh
cargo run --locked -p slimevr-server -- solve-recording recordings/run.jsonl --config examples/six-point.json
```

保留所有接收事件和 tick，校验协议回复。加 `--frames` 输出全部帧。相同 journal 与配置重复执行输出逐字节一致；六台模拟设备的实际 socket 测试还验证了实时解算与回放的最终姿态完全一致。

接收层已经转换 UDP 坐标。核心使用 `server_rotation/server_acceleration` 作为统一坐标输入。

### AutoBone

```sh
cargo run --release --locked -p slimevr-server -- autobone motion.scene.jsonl \
  --config examples/six-point.json --target-height 1.7 --output fitted-pose.json
```

输入需包含稳定校准后的方向、HMD 位置和至少三帧运动；当前限制为 5000 帧；缺失身体部位按训练骨架的回退规则处理。`--target-height` 是目标眼高/HMD 高度，未指定时依次使用已保存眼高、设置选择的骨架高度或录制最高 HMD 位置。`--settings settings.json` 可调整训练轮数、目标权重、随机种子、异常帧过滤与接受阈值。

训练使用校准后、未滤波的方向和未经过 LegTweaks 的 FK，以 FK 运动误差驱动比例拟合。输出包括初始/最终误差、每轮统计、拟合骨长和 `accepted`。仅接受结果可写入 `--output`，且只创建新文件；输出路径使用独占创建。未提供输出路径时仅报告结果。`--config` 也支持原版 `vrconfig.yml` / `.yaml`；此时默认训练参数取自 `autoBone`，显式 `--settings` 优先，输出为保留其他原版字段的 YAML。输出扩展名为 `.yml` / `.yaml` 时也使用 YAML。

AutoBone 使用专门录制的连续运动。六点演示 scene 用于校准、暂停和动作切换检查，其默认训练结果被接受阈值拒绝。合成固定脚踝测试验证了误差下降、目标高度保持与重复训练一致。

## 模块与行为

| 模块             | 已实现内容                                                                                                             |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `math.rs`        | ktmath f32 四元数/向量运算、log/exp/pow、interpQ/interpR、符号连续、YZX 欧拉角与基向量                                 |
| `calibration.rs` | Full/Yaw/Mounting reset、手动安装变换、航向平滑过渡、手臂 Back/Forward/T-pose up/down 与手指安装分支                   |
| `filtering.rs`   | NONE、SMOOTHING、PREDICTION，采样与 tick 分离、六次增量窗口、滤波复位                                                  |
| `skeleton.rs`    | 躯干/腿/肩/手臂/手/手指，缺失输入回退、Extended Spine/Pelvis/Knee，HMD 根与反向控制器手臂链，65 根骨与 11 个计算追踪器 |
| `constraints.rs` | twist/swing、球面限制、铰链与松铰链，原版部位限制及遍历范围                                                            |
| `legs.rs`        | 接触状态与有界历史、地面裁剪、滑步修正、足底贴地、脚尖吸附、膝/髋位置修正与质心                                        |
| `alignment.rs`   | 静止/近期静止检测、锁定方向、姿态分类、中心/邻居误差、按 IMU/磁力计选择的航向修正速率                                  |
| `localizer.rs`   | 无外部定位时的脚/质心/坐姿参考与根位置估计，保留下一帧 FK 才体现修正的时序                                             |
| `flex.rs`        | 电阻或角度输入、动态范围、反向电阻与最小/最大复位、手指链相对上一帧手方向的合成                                        |
| `gestures.rs`    | tap 触发复位、动作抑制、控制器测地面/HMD 测眼高的状态机、稳定性与超时                                                  |
| `autobone.rs`    | 七类误差目标、归一化骨长、骨贡献调整、候选接受、异常帧过滤、Kotlin 种子随机顺序与离线拟合                              |
| `velocity.rs`    | 原版派生线速度窗口、最终计算位置求导和失效/恢复，默认关闭                                                              |
| `pose.rs`        | 身体绑定、状态/会话、复位调度、暂停、全部算法顺序、配置导出、阶段诊断                                                  |

`PoseSnapshot` 保留原始、校准、滤波和最终方向、数据 age、骨架、接触历史当前帧、对齐状态、下一帧定位根、flex 和身高校准状态。`skeleton.bones` 是约束后的 FK；脚部后处理修改 `skeleton.computed` 中的追踪器位置/方向，因此显示最终追踪输出应读取 computed。

延续参考时序：没有躯干输入时使用上一帧颈部 yaw；手指 flex 使用上一帧手世界方向；控制器根的反向手臂使用独立树，约束遍历作用于头部根所在分量。主解算调用 `updateWithConstraints(false)`，tracker 约束反馈保持关闭；普通 `updatePose` 使用 FK 和该约束遍历路径。

`TimedOut` 可继续提供方向，`Disconnected/Error` 排除。姿态 age 按旋转输入更新时间计算。新设备从初始校准状态开始；已知 UDP 设备重新握手时保留运行校准，并清理动态方向、位置、滤波和速度历史。其他来源按自身会话规则重建。显式保存的 mounting 修正可在核心加载时恢复。加速度世界变换使用原始方向，沿用参考语义，使用接收层的统一坐标方向。

## 配置与库 API

`PoseConfig` JSON 使用 snake_case，支持：

- `bindings`：设备身份、sensor ID、身体部位和可选 mounting；重复身份/部位拒绝。
- `filter`、`skeleton`、`arms_reset_mode`、复位延迟与 yaw 平滑时间。
- `legs`、`alignment`、`localizer`、`taps`；对齐、定位与 tap 默认关闭，需显式开启。关节约束和基本脚部修正默认开启。
- `imu_types`、`magnetometers`、`flex_resistance`、`flex_angles`；实时 SensorInfo 自动更新对应信息。
- `reset_hmd_pitch`、`reset_mounting_feet`：Full reset 的 HMD 俯仰修正，以及默认 Mounting reset 是否包含脚部。
- `save_mounting_reset`、`saved_mounting_resets`、`hmd_height`。启用安装保存后，导出的修正可在重连或重新创建核心时恢复。

`PoseConfig` JSON 用于库接口和场景工具，日常服务配置使用原版 YAML，前端通过 SolarXR 修改。未知字段、非法数值、重复绑定和倒退时钟返回错误。调用者使用 `export_config()` 获取完整配置并管理保存，Service 提供后台 YAML 持久化，AutoBone CLI 提供新文件输出。

主要 API：`configure`、`restore_sample`、`reset_selected`、`set_leg_overrides`、`PoseEngine::new`、`ingest/sample`、`set_head/clear_head`、`set_controller/clear_controller`、`set_flex`、`reset`、`set_paused`、`tick`、`snapshot`、`motion_frame`、`export_config`。

`start_height_calibration` 要求头显和控制器位置。完成后自动按原版比例重置骨长并保存眼高；`apply_height` 可显式应用已测眼高。它替换全部骨长/偏移默认值，保留模型开关与插值比例；髋宽/肩宽等原版不随高度缩放的参数恢复默认值。输入高度按眼高语义解释。

scene 逐行 JSON，可使用：

```json
{"type":"head","at_ms":0,"rotation":{"w":1,"x":0,"y":0,"z":0},"position":{"x":0,"y":1.7,"z":0}}
{"type":"controller","at_ms":0,"body":"left_hand","rotation":{"w":1,"x":0,"y":0,"z":0},"position":{"x":-0.3,"y":1.1,"z":-0.4}}
{"type":"reset","at_ms":0,"kind":"full"}
{"type":"tick","at_ms":0}
{"type":"pause","at_ms":100,"paused":true}
```

还有 `sample`、`input`、`flex`、`height_calibration` 事件。全部事件按单调时间排列，首 tick 的 dt 为 0。UDP UserAction 2/3/4/5 分别触发 Full/Yaw/Mounting 或暂停；Full/Mounting 默认延迟三秒，Yaw 立即调度，下一个 tick 执行。

校准字段与 AutoBone 统计契约见 [说明](../docs/rust-calibration-autobone.zh-CN.md)。

## 验证与边界

```sh
cargo fmt --package slimevr-core --package slimevr-server -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

核心 fixture 含 99 组参考，另有 14 组 AutoBone 完整训练对照，实际运行仓库 Kotlin 数学、滤波、复位、骨、约束、LegTweaks、StayAligned、Localizer、flex 和 AutoBone 随机顺序/完整训练；外部依赖隔离，受控时钟替代墙钟。普通 Rust 测试读取已提交的 fixture。完整记录与重建方法见 [验证 MD](../docs/rust-core-validation.zh-CN.md)。

骨架对照检查 2990 个位置、1495 个方向；最大差异约 0.0000149 mm 与 0.00000683°。这组统计覆盖限定 FK 输入的移植误差，实机精度按真实动作和定位参考测量。AutoBone 已按逐轮统计、骨长、最终高度、筛选和接受结果对照原版完整训练；flex 使用 Kotlin 差分，tap 和身高校准主要使用状态测试。训练参考使用显式服务 / 录制适配器，PFS / PFR 格式由独立字节参考验证。

明确的运行策略：脚部修正要求有效世界锚点或经 Full reset / Localizer 初始化的地面；速度积分要求正 dt；身高校准等待控制器朝向时也受超时限制。tap 默认关闭。reset-history drift compensation 按参考路径保持关闭。温漂与设备融合由固件及其校准机制负责。

CI 在 Linux / Windows 检查后端。前端通信、SteamVR 协议 2 和配置共用当前服务；真实动作、设备和 SteamVR 输出按实机清单记录。

可选派生速度已接入最终计算位置、SteamVR、原版 YAML 与现有设置页面；边界与阶段差异见 [派生速度](../docs/rust-steamvr-bridge.zh-CN.md#派生速度)。

剩余日常流程、外围功能与集中测试场景见 [功能状态](../docs/rust-feature-status.zh-CN.md)。AutoBone 已逐轮推送统计及当轮骨长。
