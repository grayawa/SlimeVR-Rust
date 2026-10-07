# Rust 算法核心验证记录

日期：2026-10-04。参考提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。结果描述相同合成输入下的移植差异及功能验证，不代表实机追踪精度。

## Kotlin 参考如何生成

实际编译运行 ktmath、QuaternionMovingAverage、TrackerResetsHandler、InterpolationHandler、Bone、TransformNode、Constraint、LegTweaks/LegTweaksBuffer、StayAligned 及其姿态/拓扑访问器、Localizer 、TrackerFlexHandler 及 Tracker 派生速度方法。HumanSkeleton 的骨架装配、头/躯干/腿/手臂/手指更新、扩展模型和偏移函数原样提取；约束声明同样从源码提取。

外部 server/tracker 对象使用显式 stub。LegTweaks 与静止检测的原始墙钟替换为受控时钟；原始源码、生成源码、stub 与方法列表均保存 SHA-256。AutoBone 的 `PoseFrameIterator.randomIndices` 使用原方法和 Kotlin seeded Random 执行。

没有启动完整 Java 服务。另一个生成器执行原版 AutoBone 完整训练循环、七类目标、帧播放器、统计及 FK；服务对象、录制容器和配置使用显式适配器，不读取 PFS/PFR。正常 Rust 测试直接读取两个 golden JSON，无需 JVM。

## 参考集合

| 范围 | 组数 | 内容 |
| --- | ---: | --- |
| 数学 | 11 | 乘法/逆/单位化、幂、插值分支与外推、q/-q、YZX yaw、向量旋转、YXZ 奇异角 |
| 滤波 | 4 | 三种模式、变化 dt、符号翻转、六次增量窗口、reset |
| HMD 俯仰复位 | 3 | computed HEAD、开启/关闭、重复复位及后续姿态 |
| 身体复位 | 8 | 胸/髋/大腿/小腿 Full/Yaw/Mounting 与 yaw 过渡 |
| 手臂/手指复位 | 16 | 四个部位 × Back/Forward/T-pose up/down |
| 单帧全身骨架 | 19 | 六点与缺失输入、有/无头显锚点、输出偏移、扩展开关，以及控制器根/关节约束组合 |
| 骨架时序 | 1 | 四帧头部变化与缺失输入，上一帧 neck yaw 与保留方向 |
| LegTweaks | 4 | 每组 90 帧，分别检查地面、滑步、脚部旋转与组合处理，以及加速度和接触历史 |
| StayAligned | 4 | 600 帧原用例；站立、坐姿、躺姿各 360 帧非零左右 yaw 偏移 |
| Localizer | 1 | 240 帧，无头显的站立/行走，脚部后处理与下一帧根位置 |
| AutoBone 随机顺序 | 5 | 1/16/31/48/64 帧，分别四轮 Kotlin 种子随机顺序 |
| 派生速度 | 2 | 有效时间窗口端点、零 dt、关闭/恢复、位置丢失和历史重置 |
| computed 来源复位 | 8 | Head / Chest / 手臂的 Full / Yaw，按原非 IMU 能力处理 |
| flex | 9 | 原 TrackerFlexHandler 的轴、范围、反向电阻与手指旋转 |
| **核心集合总计** | **95** | **包含持续状态与多帧场景，组数不是总帧数** |

另有 **14 组 AutoBone 完整训练参考**：默认训练、初始误差轮、全部目标、估计高度、异常帧过滤、拒绝结果、启用约束、录制高度与骨架高度，以及缺少单 / 双小腿、单腿、仅躯干、仅 HMD。逐轮比较统计和骨长，比较最终高度、骨长、筛选后帧数及接受结果。

## 差异

数学、校准、滤波和后处理按输出分量使用 `3e-6 × (1 + abs(expected))` 容差。接触/静止状态、随机序列等离散值要求完全一致。

骨架另外以欧氏距离和 `Quaternion.angleToR` 统计，处理 q/-q 等价。以下指标仅包含 FK 骨架集合，不把脚部后处理和对齐测试混作同一误差统计。百分位使用排序后向上取整秩。

| 指标 | 数量 | 最大值 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| 位置差异（mm） | 2990 | 0.0000149012 | 0 | 0 |
| 方向差异（°） | 1495 | 0.00000683019 | 0.00000341509 | 0.00000354599 |

骨架测试要求每个位置 `<1 mm`、方向 `<0.05°`；所有参考帧通过。后处理对照覆盖原始/修正位置、方向、速度、加速度、接触状态、质心、数值状态及根位置。

## 功能与运行链路

当前共 112 项 Rust 测试，涵盖核心参考、算法 / 校准状态、AutoBone、接收协议、串口 / OTA / HID、OSC、外部来源、场景回放、BVH、YAML 和 SteamVR。另有 17 项 Node 前端 / 桌面测试及 5 项 Tauri 测试，见 [实施交接](rust-completion-worklog.zh-CN.md)。

- 实际 UDP socket：六台模拟设备、双 sensor、重复/乱序、紧凑包、截断包、回复校验；在线解算与 journal 回放的最终 `PoseSnapshot` 完全一致，重复回放逐字节一致。
- 核心状态：采样/tick 分离、过期姿态、缺失来源、重启会话、复位延迟、暂停、安装修正导出/加载/重连恢复、带位置头部与外部来源移除。
- 持续运行：4000 帧合成行走，脚部历史不超过 11 帧，位置/旋转保持有限。
- AutoBone：48 帧固定脚踝合成运动，拟合误差下降、目标高度保持、相同种子重复结果一致；CLI 写出的 JSON 场景配置和原版 YAML 均可加载，YAML 导出保留其他字段，拒绝结果不写入，现有文件不覆盖。
- flex/tap/身高：反向电阻、手指相对手旋转、metadata 路由与断连排除、分离 tap 峰和动作抑制、地面/眼高稳定性、超时、完成后骨长更新与配置导出。

AutoBone 完整训练对照已覆盖上述 14 组受控录制；tap 和身高校准仍以功能验证为主，flex 已有 Kotlin 差分。普通训练误差 / 高度使用 `3e-6 × (1 + abs(expected))`，逐轮及最终骨长绝对容差 `0.1 mm`；仅 HMD 用例分别使用 `1e-5 × (1 + abs(expected))` 和 `1 mm`，该用例最终小腿差约 `0.6 mm`。候选接受会放大 f32 FK 舍入差异，离散结果仍要求一致。这不证明所有录制输入下完全等价；详细修正见 [校准与训练对照](rust-calibration-autobone.zh-CN.md)。

Linux 测试和 Clippy 无警告通过；Windows GNU 交叉编译通过，尚未在 Windows 执行。已有 Web / Linux Tauri 前端通信联调；真实 tracker 和 SteamVR 尚未验收。当前 4 ms 实时 tick 是调度设置，没有据此宣称硬实时、实机延迟或原版千赫兹循环等价。

## 复现

```sh
cd server-rust
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/generate-core-golden.py
python3 tools/generate-autobone-golden.py --skip-core-build
SLIMEVR_CORE_METRICS=/tmp/core-metrics.json cargo test -p slimevr-core --test golden --locked -- --nocapture
```

参考重建需要 Python 3、JDK 17+ 与首次访问 Maven Central。`--javac-java-modules` 可在只有 JRE 的环境指定完整 JDK jmods，详见生成器帮助。`SLIMEVR_CORE_METRICS` 只指定测试统计输出，不影响算法。

使用方式与实现边界见 [算法核心说明](../server-rust/README.core.zh-CN.md)。后续验收应使用真实六点布局录制，包含 HMD/控制器、转身、行走、蹲/坐、复位、断网、重启与热机前后；保持原始包、事件时钟和配置同步。

BVH 导出新增实际 Kotlin 导出器、原样骨架装配与 TickReducer 对照，见 [BVH 导出验证](rust-bvh-export.zh-CN.md)。

派生速度已接通原版配置和 SteamVR，并验证真实 CLI 在线发布帧与回放逐帧一致；阶段时序差异见 [派生速度说明](rust-derived-velocity.zh-CN.md)。
