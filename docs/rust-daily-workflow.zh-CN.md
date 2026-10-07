# Rust 日常 Wi-Fi／SteamVR 流程实施与验收

日期：2026-10-04。原服务参考提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

本轮完成 AutoBone 文件、敲击分配、磁力计控制和 SteamVR 驱动管理。配置仍使用原版 `vrconfig.yml`／`vrconfig.yaml`，没有增加日常 JSON 配置。外围协议现已接入，仍有实机验收和数值 / 时序边界，见 [剩余功能差异](rust-remaining-feature-gaps.zh-CN.md)。

## AutoBone 文件

- 录制完成或提前停止且至少有三帧时，写入 `AutoBone Recordings/LastABRecording.pfs`，完成消息在文件落盘后发送。原子替换最后一次录制。
- `autoBone.saveRecordings: true` 另外生成 `ABRecording1.pfs` 等编号文件；设置已支持原版 SolarXR 读写。SAVE 请求也生成编号文件，不覆盖已有文件；录制中的 SAVE 等待本次录制结束，取消／不足三帧停止会结束待保存操作并报告失败。
- PROCESS 优先使用 `Load AutoBone Recordings` 中按文件名排序的 `.pfs`／`.pfr`，其次使用当前内存录制；重启后可回退到 `LastABRecording.pfs`。最后一项是 Rust 增加的复用行为。多个导入录制分别训练并推送进度，最终应用最后一份结果，沿用原版行为；任意失败或被拒绝的训练不能应用。
- 默认配置路径沿用 SlimeVR 平台目录。指定 `--config` 时，两个录制目录跟随该配置文件所在目录；原 Java 按平台默认配置目录处理。
- CLI `autobone` 同样支持 PFS/PFR 输入；GUI 生成 PFS，解码器与编码器均支持 PFR。目录导入有上限：64 个文件，每文件 16 MiB，256 个 tracker、5000 帧；错误格式、非法数值及不满足训练要求的文件明确报错，原版可能跳过部分损坏文件。
- 编码沿用 Java 大端整数／float、modified UTF-8 和 TrackerPosition ID，包含间隔、部位、旋转、位置、加速度及原始旋转标志。原版文件中的可选字段可保留；当前 GUI 录制主要保存校准后的未滤波旋转和位置，不额外录入全部调试遥测。加载沿用原播放器的短 tracker 最后一帧和缺省字段继承语义。

PFS/PFR 的字节参考运行实际 Kotlin `PfsIO`、`PfrIO`、`PfsPackets` 及 TrackerFrame getter；仅录制容器、配置和日志采用显式适配器。测试涵盖中文、空字符、emoji、全部标志、手臂／手指、空帧及长度不同的 tracker。fixture 记录参考提交、实际源码和适配器哈希，普通 Rust 测试无需 JVM。

原版参考生成：先运行 `server-rust/tools/generate-autobone-golden.py` 生成共享核心参考类，再运行 `server-rust/tools/generate-pose-recording-golden.py`。默认缓存为 `/tmp/slimevr-udp-oracle`，需要 Python、Java 和首次 Maven 下载。

## 敲击辅助分配

原有分配向导的 `TapDetectionSettings.setupMode` 已接通。按原版逻辑检测两次敲击，返回 `TapDetectionSetupNotification`，其中包含稳定的 device ID 和实际 sensor ID，支持 sensor 0 与未分配 tracker。

分配模式暂停普通敲击复位。检测器按设备、sensor 和连接会话隔离；断线、加速度过期、模式／移动阈值改变会清理对应状态。移动背景阈值沿用现有 TapDetector。模式写入原版 `tapDetection.setupMode`，与原服务一样由前端切回普通模式。

## 磁力计控制

全局设置使用 `server.useMagnetometerOnAllTrackers`，单 tracker 偏好使用原版 `trackers["udp://MAC/sensor"].shouldHaveMagEnabled`。实际启用条件为全局开关与单 tracker 偏好的交集；未设置的单 tracker 偏好默认开启，全局默认关闭。删除设备同时清除其偏好，保留其他 tracker 来源。

修正原 SensorConfig 位语义：bit 1 表示支持，bit 0 表示启用。SolarXR TrackerInfo 返回 NOT_SUPPORTED／DISABLED／ENABLED，使原有磁力计控件能够显示。

命令沿用 UDP 包 25：12 字节头、sensor byte、配置类型 u16 大端和布尔 byte，MAG_ENABLED 类型为 1；等待包 24 ACK 后更新实际报告状态并回复前端。未 ACK 的请求最多等待 10 秒，断线／重启报告未确认；希望配置保留在 YAML，并在设备新会话中重新下发。超时后，同一 sensor 等待旧 ACK 或重连，避免无事务号协议的迟到 ACK 错认新的操作。协议 ACK 不含状态值，无法完全消除固件重复 ACK 等歧义，需真实固件验收。

设备命令加入 journal 版本 3，回放验证相同命令、实际 UDP 输出和 ACK 后状态；仍可读取版本 1、2。

## SteamVR 驱动管理

Tauri 资源包含官方 SlimeVR OpenVR Driver **v6.0.0** 的 Windows x64、Linux x64／aarch64 发布包和许可证。`gui/scripts/fetch-steamvr-drivers.py` 固定发布 URL 与下载包 SHA-256；`pnpm tauri:dev`、`tauri:build`、`tauri:rust` 自动准备资源，已缓存时不重复下载。直接调用 Tauri CLI 前需先执行 `pnpm --dir gui tauri:drivers`。

后端读取 OpenVR 的 `openvrpaths.vrpath`，调用原版 `vrpathreg finddriver slimevr`／`adddriver <目录>`。已有注册保持原状；发现 SteamVR 的手动安装目录或工具返回异常时停止注册并报告错误。Tauri 将打包资源中的平台驱动路径传给 Rust。macOS 不自动注册这些平台的驱动。

使用 SteamVR 本地 HTTP 服务的 `/drivers/list.json` 读取实际安装、启用和安全模式状态。原前端 EnableSteamVRDriverRequest 调用 `/drivers/unblock` 和 `/drivers/setenable`，带原版 Referer，验证启用结果后请求 `vrmonitor://restartsystem`。SteamVR 未运行时保持“未知”，不假定驱动已安装。

驱动状态进入原 `TrackingChecklistSteamVRDisconnected`；注册错误也保留在 backend_info，使稍后连接／重连的前端可看到。ServerInfos 的 localIp 返回显式绑定地址，未指定地址则选本机 IPv4 网卡；无可用 LAN 地址时回退到 loopback。

后端 CLI 新参数：

| 参数 | 用途 |
| --- | --- |
| `--steamvr-driver <目录>` | 明确指定带 driver.vrdrivermanifest 的驱动目录 |
| `--steamvr-runtime <目录>` | 覆盖 SteamVR runtime 自动定位 |
| `--no-driver-install` | 保留桥接，跳过自动注册 |
| `--no-steamvr-restart` | 启用驱动后不自动请求重启 |
| `--steamvr-http <本地 HTTP 地址>` | 覆盖管理端点，默认 127.0.0.1:27062；仅允许 loopback |

Bindings Provider 已从原版源码构建并随包，仍可指定已有安装路径。Vive / Tundra 身体输入、协议 1 feeder、overlay 和 VRChat 配置均已接通，详见 [交接记录](rust-completion-worklog.zh-CN.md)。

## 验证与集中实测

自动验证包括 112 项 Rust 测试、17 项 Node 前端／桌面测试、5 项 Tauri 单元测试，以及 Clippy、TypeScript、修改页面 lint、Vite 构建、Linux release 和 Windows GNU 后端编译。新增自动用例覆盖：

- 原版 PFS/PFR 双向字节兼容、编号不覆盖、原子最后录制、导入校验、SAVE 等待与重启后训练。
- 原 TypeScript SolarXR 的真实 WebSocket／UDP 敲击分配、sensor 0、ACK 前不完成、实际状态和 YAML 持久化。
- 可控时钟下超时、迟到 ACK、重连重新下发、非法命令组无部分发送及设备命令回放。
- 模拟 SteamVR HTTP 的原路由、JSON、Referer、失败反馈，以及 vrpathreg 注册／已有安装保护。
- 前端可选 false／0 字段保留：修正原生成代码省略这些值的问题，同时保留空消息 union 的正确编码。

还没有真实追踪器／SteamVR、Windows 原生运行、macOS 安装包验收。集中实测时先复用现有 Wi-Fi tracker：确认 YAML 加载、批准／分配、Full／Mounting、追踪和重连；再测试敲击分配、磁力计 ACK／超时、AutoBone 保存／重启／导入，最后检查 SteamVR 首次注册、安全模式启用与 Tauri 退出。完整清单见 [功能差异文档](rust-unified-hardware-test.zh-CN.md)。
