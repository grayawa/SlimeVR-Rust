# Rust 后端与 Web / Tauri 前端联调

本阶段把已有 React 界面接到 Rust 的 UDP 接收与算法核心。界面继续使用仓库现有 SolarXR FlatBuffers 协议和生成绑定，设备列表、骨架预览和设置页面复用现有数据模型。参考上游提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

## 启动

在仓库根目录执行，需要 Rust 1.88+、Node.js 和 pnpm：

```sh
pnpm rust:build
./server-rust/target/release/slimevr-server listen \
  --api-bind 127.0.0.1:21110 --config ./vrconfig.yml \
  --pose-output-ms 1000
```

Windows 使用 `server-rust\target\release\slimevr-server.exe`。UDP 默认监听 `0.0.0.0:6969`，WebSocket 按上述参数监听 `127.0.0.1:21110`。另开终端执行 `pnpm web`，访问 `http://127.0.0.1:5173`。前端支持通过 `?ip=127.0.0.1&port=21110` 连接指定服务。

默认要求批准新设备：固件握手触发现有“发现新设备”弹窗，批准后固件重试握手即可进入列表。随后分配身体部位、选择安装方向并执行 Full / Mounting reset。也可提前传入多个 `--allow-device AA:BB:CC:DD:EE:FF`。测试时可显式使用 `--accept-new-devices`；自动发现可通过 `--no-discovery` 关闭。

启用 `--api-bind` 自动创建姿态核心，无需提前编写身体绑定。`--pose-config server-rust/examples/six-point.json` 可以导入初始配置，示例 MAC 必须改为实际设备。已有 `vrconfig.yml` / `.yaml` 优先，`--pose-config` 只作为全新配置的种子。启用 API 后即使不传 `--config`，也按原版路径读取和保存 YAML；`server.trackerPort` 控制 UDP 端口，显式 `--bind` 优先。

### Tauri 启动 Rust

```sh
pnpm tauri:rust:dev
```

此命令构建 Rust release 后端、SolarXR TypeScript 绑定，然后启动 Tauri 开发环境。Linux 需要 GTK / WebKitGTK 等 Tauri 系统开发依赖。

Tauri 参数：

- `--backend auto`：默认；使用发现的 Rust 可执行文件或连接已有服务。
- `--backend rust`：明确使用 Rust，找不到时显示启动错误。
- `--rust-server <路径>`：指定 Rust 可执行文件。
- `--config <路径>`：指定原版 YAML 配置文件；旧 `--rust-state` 参数仍作为别名接受。
- `--pose-config <路径>`：导入初始姿态配置。
- `--no-steamvr` / `--steamvr-endpoint <路径>`：关闭或指定 SteamVR 桥接端点。
- `--bindings-provider <路径>` / `--no-bindings-provider`：指定或关闭原版辅助程序。
- `--no-server`：连接已运行的服务。

Rust 查找范围为应用资源目录、GUI 可执行文件同目录、`--path` 目录；开发构建还查找仓库 `server-rust/target/release` 和 `debug`。本地 21110 端口已占用时复用已有服务。GUI 自己启动的子进程在正常退出时回收，外部已运行的服务不会被停止。

后端直接使用原版 `vrconfig.yml` / `.yaml`。Tauri 默认采用 `AppPaths.server/vrconfig.yml`，Linux 通常位于 `~/.config/dev.slimevr.SlimeVR/`；GUI 界面偏好仍由现有桌面适配器管理。保存保留未支持的 YAML 字段，备份上一版为 `.bak`，使用同目录临时文件和原子替换；非法配置拒绝加载，非法修改返回错误并保留已接受的设置。若没有 YAML 而同目录存在旧 `rust-backend.json`，自动迁入 YAML，旧文件保留且不再写入。字段映射、默认路径与兼容范围见 [原版配置复用说明](rust-config-compatibility.zh-CN.md)。

打包 Rust 后端：先构建当前操作系统的 release 可执行文件，通过宿主构建脚本打包原版 OpenVR helper、驱动、许可文件和后端：

```sh
pnpm tauri:rust:build
```

Windows 改用 `src-tauri/tauri.rust.windows.conf.json`。两个配置分别打包无扩展名和 `.exe` 后端；发布各平台安装包时应在对应平台构建。旧 Java 打包配置已移除。

## 已接通的功能

| 现有前端操作                | Rust 行为                                                                  |
| --------------------------- | -------------------------------------------------------------------------- |
| 设备发现 / 批准 / 删除      | MAC 准入、按设备限流的通知、移除绑定和当前接收状态                         |
| 设备与追踪器列表            | 真实状态、四元数、已接收的遥测、自定义名称；缺失电量不伪造为 0             |
| 分配角色 / 安装方向         | 更新 `PoseConfig`，为新绑定恢复最近样本并保留原始接收时间                  |
| 骨架预览                    | 推送约束后骨架；11 个计算追踪器使用脚部后处理后的最终位置和方向            |
| Full / Yaw / Mounting reset | 延迟、进度、完成通知、按身体部位复位；正常设置变更保留未改绑设备的运行校准 |
| 暂停 / 恢复                 | 控制姿态核心并广播当前状态                                                 |
| 手动人体比例 / 恢复比例     | 21 类骨长和偏移，校验后保存并重新配置骨架                                  |
| 滤波 / FK / 腿部修正 / 定位 | 映射已有设置表；人体比例页面临时关闭修正不会写入永久设置                   |
| StayAligned / relaxed pose  | 开关、姿态参数、采集与清除参考姿态                                         |
| tap / 安装重置保存          | 单独启用各类 tap、延迟、次数与目标部位；安装修正按开关保存                 |
| 控制器 / HMD 测眼高         | 启动、取消、状态和测量结果；需要外部位置输入                               |
| AutoBone                    | 运动录制、停止 / 取消录制、后台训练、训练结果、应用通过接受阈值的骨长      |
| 派生速度 | 原版开关、YAML 保存、最终计算追踪器速度和 SteamVR 可选字段 |
| SteamVR | 头显/控制器输入、最终计算追踪器输出、自动/手动共享、连接状态、原版本地 RPC |
| BVH 录制 | 原版兼容骨架、米尺度、ZXY 通道与 100 Hz 采样，文件收尾、状态同步和保存提示 |
| 断线重连                    | 恢复读取与订阅；复位、分配等用户修改不会自动重发                           |

前端显式处理 Rust 的 `backend_info` 能力通知、`backend_error` 错误通知和 BVH 的 `backend_file_saved` 保存通知。它们只作为连接辅助消息，已有数据和 RPC 仍是二进制 SolarXR。查询响应保留事务编号，实时状态变化广播给所有已连接前端。

WebSocket 最多 16 个客户端，每个客户端最多 8 个订阅，单个输入消息上限 64 KiB，数据订阅最短间隔 10 ms。前端默认约 10 Hz 设备数据和 40 Hz 骨架；核心默认每 4 ms 解算，发布频率和日志频率分别控制。慢客户端通过最新状态快照、发送超时和有界事件队列处理。

## 外部头显与控制器输入

SteamVR 驱动协议 2 的头显/控制器输入和计算追踪器输出已接入，Linux/Windows API 模式默认启用，详见 [SteamVR 桥接说明](rust-steamvr-bridge.zh-CN.md)。也保留独立的 WebSocket 文本输入，时间由服务端单调时钟分配；单位为米，使用核心坐标 `+X` 右、`+Y` 上、`+Z` 后：

```json
{"type":"pose_input","body":"head","rotation":{"w":1,"x":0,"y":0,"z":0},"position":{"x":0,"y":1.7,"z":0}}
{"type":"pose_input","body":"left_hand","rotation":{"w":1,"x":0,"y":0,"z":0},"position":{"x":-0.3,"y":1.1,"z":-0.4}}
{"type":"pose_clear","body":"left_hand"}
```

支持 `head`、`left_hand`、`right_hand`。外部源使用虚拟设备 ID 0，真实 UDP 设备使用 1..254；传感器编号 0 在 GUI 中也可正常查找。停止外部源时桥应发送 `pose_clear`。没有外部 HMD 世界位置时，普通 UDP 只提供相对骨架，不能完成依赖头显位置的 AutoBone；测眼高还需要控制器位置。测眼高的有效 HMD 高度范围保持核心的 1.2..1.936 m。

## 录制与验证

`--record <新文件.jsonl>` 记录 UDP、解算 tick、初始姿态配置、前端控制以及准入/删除变化。纯接收录制保持 v1；带算法的录制使用 v2。命令扩展使用 v3，HID 输入使用 v4；当前回放器支持 v1–v4，旧回放器不能读取新格式。

```sh
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
cargo clippy --manifest-path server-rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo build --manifest-path server-rust/Cargo.toml --locked
pnpm --dir gui test:backend
pnpm --dir gui test:desktop
pnpm --dir gui typecheck
pnpm --dir gui web:build
```

`test:backend` 使用实际 Rust CLI、六个 UDP socket、WebSocket 和前端生成的 TypeScript FlatBuffers 绑定，验证批准、传感器编号 0、分配和命名、骨架、复位、设置和骨长修改、暂停、外部 HMD、组件订阅掩码、非法请求回滚、临时腿部设置、删除设备、重启保存与实时/回放一致。新增原版 YAML 联调覆盖已有绑定、读取设置、修改和恢复默认值写回、备份及未知字段保留。可通过 `SLIMEVR_RUST_BINARY` 指定待测可执行文件。另有核心测试覆盖热配置保留校准、原始样本 age、局部复位和临时设置。当前 112 项 Rust、17 项 Node 前端 / 桌面、5 项 Tauri 测试通过；Linux Tauri 的启动、设置保存、正常关闭回收及 Linux Debian 打包已验证。Windows GNU 后端交叉编译通过。

驱动安装、OSC / OSCQuery / VMC、VRChat、USB HID、串口 / 固件、磁力计、快捷键、overlay、Discord 及原 AutoBone SAVE / PFS-PFR 录制复用均已接通。原版接口支持取消录制，没有独立取消训练接口。实现与集中实机验收见 [交接记录](rust-completion-worklog.zh-CN.md) 和 [测试清单](rust-unified-hardware-test.zh-CN.md)。Windows / macOS 原生运行仍需验收。

BVH 已按原版导出方法接通。浏览器默认写入后端状态目录的 `recordings/`，桌面沿用保存对话框；Tauri 自有 Rust 后端通过父管道 EOF 正常收尾。导出规则、采样和参考验证见 [BVH 使用说明](rust-bvh-export.zh-CN.md)。

## 代码入口

- `server-rust/crates/slimevr-server/src/steamvr/`：驱动 Protobuf、平台 IPC、共享、重连及辅助进程生命周期。
- `server-rust/crates/slimevr-server/src/config.rs`：原版 YAML 字段适配、版本迁移、未知字段保留与原子保存。
- `server-rust/crates/slimevr-server/src/api/mod.rs`：WebSocket、RPC 调度、配置、校准与 AutoBone 生命周期。
- `server-rust/crates/slimevr-server/src/api/protocol.rs`：SolarXR 编码、组件掩码、设备与骨架数据。
- `server-rust/crates/slimevr-server/src/api/settings.rs`：已有设置表与 Rust 配置转换。
- `server-rust/crates/slimevr-server/src/runtime.rs`：接收 / 核心 / API 的单一事件循环。
- `gui/src/hooks/websocket-api.ts`：消息分发、错误通知与读取订阅恢复。
- `gui/src/platform/solarxr.ts`：协议解码、后端通知和有界订阅缓存。
- `gui/src-tauri/src/server.rs`：后端选择、定位、启动和退出回收。

派生速度已实现，算法、时序与校验见 [说明](rust-derived-velocity.zh-CN.md)。

HMD 俯仰复位和默认脚部安装校准已通过 `extended_calibration` 能力开放原设置页面，沿用 `resetsConfig`。`extraYawCorrection` 按原版作为废弃字段忽略；AutoBone 初始误差与骨架高度设置可读写，详见 [校准与训练对照](rust-calibration-autobone.zh-CN.md)。
