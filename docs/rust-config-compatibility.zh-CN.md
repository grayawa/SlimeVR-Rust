# Rust 直接复用原版 SlimeVR 配置

日期：2026-10-04。参考上游提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

Rust 后端直接读取并写回原版 `vrconfig.yml` / `vrconfig.yaml`。设备绑定和算法设置以原版 YAML 字段为准。前端修改设置、分配设备、保存安装校准、应用 AutoBone 结果时，更新同一份 YAML。

## 启动与配置位置

从仓库根目录构建并指向现有配置：

```sh
pnpm rust:build
./server-rust/target/release/slimevr-server listen \
  --api-bind 127.0.0.1:21110 --config /path/to/vrconfig.yml
```

Windows 使用 `.exe`。Tauri 使用 `pnpm tauri:rust`，可传入 `--config <路径>` 覆盖默认文件位置。

启用 `--api-bind` 而未指定 `--config` 时，CLI 自动采用原版默认位置：工作目录已有 `vrconfig.yml` 或 `config/` 目录时使用工作目录，否则查找本地 `vrconfig.yaml`，再使用平台配置目录：

| 平台    | 默认文件                                                                                                     |
| ------- | ------------------------------------------------------------------------------------------------------------ |
| Windows | `%APPDATA%/dev.slimevr.SlimeVR/vrconfig.yml`                                                                 |
| Linux   | `$XDG_CONFIG_HOME/dev.slimevr.SlimeVR/vrconfig.yml`，未设置时为 `~/.config/dev.slimevr.SlimeVR/vrconfig.yml` |
| macOS   | `~/Library/Application Support/dev.slimevr.SlimeVR/vrconfig.yml`                                             |

Tauri 使用其原有 `AppPaths.server` 服务目录中的 `vrconfig.yml`。两种启动方式都在 `.yml` 不存在而 `.yaml` 存在时复用后者。纯 UDP 接收模式不带 API 或显式配置时，仍可独立运行。

`server.trackerPort` 控制 UDP 监听端口，显式 `--bind` 优先；WebSocket 监听地址仍由 `--api-bind` 指定。已有 YAML 优先于 `--pose-config`，后者只为全新配置提供初始姿态。

## 字段映射

| 原版节点                              | Rust 使用范围                                                                      |
| ------------------------------------- | ---------------------------------------------------------------------------------- |
| `knownDevices`                        | 已批准的 UDP 设备 MAC                                                              |
| `trackers`                            | UDP sensor 的身体部位、自定义名称、安装方向、保存的安装复位及 shouldHaveMagEnabled |
| `skeleton.offsets`                    | 21 类骨长和偏移                                                                    |
| `skeleton.toggles` / `values`         | 已实现的扩展脊柱、骨盆、膝盖、手臂、关节约束、腿部修正、定位与七项比例参数         |
| `skeleton.hmdHeight` / `floorHeight`  | 眼高，按原版两者之差换算                                                           |
| `filters`                             | 无滤波、平滑、预测及强度                                                           |
| `resetsConfig`                        | 身体/手臂复位模式、HMD 俯仰、默认脚部安装复位、延迟、yaw 过渡和安装复位保存        |
| `legTweaks`                           | 修正强度及始终使用地面裁剪                                                         |
| `stayAlignedConfig`                   | 自动对齐与站立、坐姿、躺姿参考参数                                                 |
| `tapDetection`                        | 各类 tap 开关、次数、延迟、目标身体部位及 setupMode 敲击分配                       |
| `autoBone`                            | 训练参数（含 calcInitError/useSkeletonHeight）、录制长度和间隔                     |
| `bridges.steamvr`                     | 自动共享、标准计算追踪器角色开关                                                   |
| `velocityConfig.sendDerivedVelocity`  | 派生速度开关，沿用原版缺省关闭                                                     |
| `server.trackerPort`                  | UDP 接收端口                                                                       |
| `server.useMagnetometerOnAllTrackers` | 全局磁力计开关，与单 tracker 偏好共同控制原版设备配置命令                          |

UDP 配置键使用原版形式 `udp://AA:BB:CC:DD:EE:FF/0`，末尾数字为 sensor ID；身体部位使用 `designation: body:chest` 等原版值。MAC 统一为大写，已有小写配置键按同一 MAC 复用。缺少安装方向时采用原版各身体部位的默认安装方向，。

SteamVR 头显/控制器来源由驱动桥自动注册，共享设置取自 `bridges.steamvr`，详见 [桥接说明](rust-steamvr-bridge.zh-CN.md)。HID、OSC / VRChat / VMC 与通用 SteamVR tracker 已接入；绑定和自定义名称使用原 trackers 条目，来源映射补充在 rust 节点。OSC、VRM、HID、快捷键、overlay 与 VRChat 配置均可通过原 RPC 修改并保存。drift compensation 字段按参考版本的关闭状态保存。

Rust 所需的稳定设备 ID、非 MAC 场景绑定及部分算法元数据放在同一文件的 `rust` 扩展节点。原版标准字段仍为标准设置的来源。Java 可忽略未知扩展节点，但 Java 再次保存文件时可能移除该节点；往返保存后以文件实际保留的 `rust` 节点恢复独有状态。

## 版本、保存与旧配置迁移

当前采用原版配置版本 15。版本 1–14 按 `CurrentVRConfigConverter.java` 的转换规则迁移，包括 tracker 数组、历史骨长命名和拆分、旧滤波/复位字段、已知设备列表及 AutoBone 默认值。没有 `version` 的文件按原版默认版本 15 处理。未来版本、无效数据及违反核心校验的配置明确报错，加载成功并通过校验后才执行保存。

保存从已加载的 YAML 树更新已支持的字段，保留未映射字段、其他来源 tracker 以及整数键映射。发生实际写入时，把上一版原始字节备份到 `<配置路径>.bak`，随后使用同目录临时文件、同步和原子替换保存。运行期保存由单个配置线程顺序执行，队列保留正在写与最新待写版本，成功回复等待对应 revision 落盘。启动时执行同步校验、迁移与标准化保存，见 [持久化边界](rust-backend-api-architecture.zh-CN.md#快照保存与-steamvr-输出诊断)。序列化器按加载后的 YAML 数据树生成新的排版。

没有 YAML 且同目录存在旧 `rust-backend.json` 时，自动迁入 `vrconfig.yml`。已有 YAML 时以 YAML 为准，旧 JSON 作为迁移来源保留。CLI 的 `--state` 与 Tauri 的 `--rust-state` 仍作为参数别名接受；显式传入旧 `.json` 路径也会切换为同目录 YAML。

`PoseConfig` JSON 用于场景与库接口，诊断 JSON 用于机器输出，JSONL 用于事件 journal。日常后端配置使用 YAML。`solve`、`solve-recording` 与 `autobone --config` 也可使用原版 YAML；AutoBone 的 YAML 输入默认从 `autoBone` 读取训练参数，输出保留其他节点，显式 `--settings` 可覆盖训练参数。

配置范围仍受当前 Rust 核心校验约束，例如身体部位绑定要求唯一、有效眼高范围为 1.2–1.936 m。同一文件由一个后端管理，当前保存基于启动时加载并由该进程更新的树。切换后端时先退出当前服务，再加载配置。

## 验证

配置适配参考原版 `ConfigManager.java`、`VRConfig.kt`、`CurrentVRConfigConverter.java`、各配置类、`TrackerConfig.kt`、`TrackerPosition` 默认安装方向及平台路径实现。

- Rust 配置测试覆盖版本 15 字段与双 sensor、旧版本迁移、未知字段和原始备份、解绑/删除、旧 JSON 迁移、非法文件保护，以及真实 CLI 默认位置和 YAML 端口。
- 真实 Rust 服务、UDP socket 和前端 SolarXR 绑定联调验证已有 YAML 设置与部位读取、修改和恢复默认值写回、保留未知字段、备份及 YAML 持久化。
- AutoBone CLI 测试验证 YAML 训练参数、完整 YAML 导出和其他字段保留，输出使用独占创建的新路径。

CI 的配置和通信检查覆盖加载、校验、修改与保存。迁移预期依据参考源码规则建立；真实配置、设备和平台路径按实机测试验收。

实现入口：`server-rust/crates/slimevr-server/src/config.rs`；配置 fixture：`server-rust/crates/slimevr-server/tests/fixtures/vrconfig-v15.yml`。前后端启动和功能范围见 [联调说明](rust-frontend-integration.zh-CN.md)。

AutoBone 的 `saveRecordings`、PFS/PFR 文件及磁力计配置／ACK 流程详见 [日常流程说明](rust-daily-workflow.zh-CN.md)。
