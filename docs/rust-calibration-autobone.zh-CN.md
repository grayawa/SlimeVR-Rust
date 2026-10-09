# Rust 校准分支与 AutoBone 训练对照

日期：2026-10-04。原版参考提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

## 校准行为

`resetsConfig.resetHmdPitch` 默认关闭。开启后，包含 HEAD 的 Full reset 按原版 `TrackerResetsHandler` 提取去除 YZX yaw 后的 pitch 修正，随后每个外部 HMD 样本应用该修正。保留原始输入用于后续复位，位置沿用输入坐标；姿态解算、身高校准、AutoBone 录制及 SolarXR 头部预览使用校准方向。HMD 修正在包含 HEAD 的 Full reset 中更新，移除头部来源清空 HMD 修正。无法提取有效旋转时明确报错。

`resetsConfig.resetMountingFeet` 默认关闭。普通 Mounting reset 沿用原版默认部位列表，开启后额外包含左右脚。脚部和手指可通过显式选择执行该部位复位。该开关作用于 Mounting reset 的默认部位选择。现有 `saveMountingReset` 可保存脚部修正。

两项设置已接入原版 YAML、SolarXR 设置读写和常规设置页面。Rust 宣告 `extended_calibration` 能力，前端据此开放原有控件；日常配置统一保存在 YAML。

## StayAligned 字段与参考

当前原版 RPC 明确把 `extraYawCorrection` 标记为 deprecated，返回 false，修改请求不读取该字段。Rust 接受并忽略该字段，返回 false，保持参考 RPC 的兼容语义。

站立、坐姿、躺姿的腿部参考 yaw 偏移已有实现。参考集合包含三组各 360 帧的非零大腿、小腿和脚部偏移对照，验证左右侧符号及逐帧状态。另有三组实际 Kotlin HMD 复位序列，覆盖 pitch/roll、连续样本、重复复位及开关变化。

## AutoBone 训练契约

- 两个训练骨架跨帧和候选保留状态，从首帧初始化；缺失部位按骨架回退规则解算。
- 统计跨轮累计，每轮 mean、SD、steps 和最终接受判断使用同一组累计观测。
- `calcInitError` 控制零调整初始轮，包含其随机序列消耗；`useSkeletonHeight` 控制动态高度估计来源。显式目标／已保存眼高优先，其次按开关选择骨架高度或录制最高 HMD 位置。
- 训练保留配置的关节约束，绕过 LegTweaks。关节约束采用录制配置的开关状态。
- 位置目标使用原版 TrackerPosition 到计算追踪器的映射：UpperChest、Hip、腿、脚、上臂和手；目标集合由这些已映射的角色组成。
- 位置目标沿用原版记录值与归一化解算值的比较方式，FK 输入位置则按训练高度缩放。归一化顺序、骨贡献、零调整跳过及候选接受行为与原版对应。

结果中的 `final_error` 表示用于接受判断的累计训练均值；`evaluation_error` 表示使用最终骨长按时间顺序重新评估的误差，便于和 `initial_error` 比较拟合改善。每个 epoch 保留对应骨长快照。GUI 实时接收每轮统计及对应骨长；应用与导出要求结果通过接受阈值。

## 参考执行范围

`tools/generate-autobone-golden.py` 原样提取实际 `AutoBone` 训练方法，编译实际帧迭代器、帧播放器、统计、骨贡献和七类误差。FK 与骨骼偏移方法也来自当前 checkout。服务、配置和录制容器是显式适配器；骨架参考保持跨帧状态。生成文件记录参考提交、实际源码及适配器 SHA-256。

十四组数据分别覆盖默认训练、初始误差轮、全部目标、动态估计高度、异常帧过滤、拒绝结果、关节约束、录制最高高度和当前骨架高度，以及缺少单 / 双小腿、单腿、仅躯干和仅 HMD；包含固定随机顺序及非随机训练。每组使用 16 帧录制，执行四轮训练；初始误差用例另外执行第零轮。

离散值要求一致：帧筛选数量、轮数、累计步数和接受／拒绝。每轮均值、SD 和高度按 `3e-6 × (1 + abs(expected))` 验证；逐轮及最终骨长按绝对 `0.1 mm` 验证；仅 HMD 用例使用 `1 mm`，均值 / SD / 高度使用 `1e-5 × (1 + abs(expected))`。七组基础数据实测最大骨长差约 `0.093 mm`、最大统计差约 `0.0000032`。严格候选比较会放大底层 f32 FK 舍入差异，因而骨长采用独立物理单位阈值。

训练参考覆盖上述完整循环与受控输入，真实录制与其他部位组合按样本验收；PFS / PFR 由独立字节参考验证，见 [日常流程实施](rust-daily-workflow.zh-CN.md)。flex 已有九组 Kotlin 差分；tap、身高校准仍主要使用功能测试；真实 SteamVR 和追踪器表现仍待验收。

## 验证与重建

后端与前端检查覆盖 HMD 状态、默认／显式安装复位、YAML 读写、真实 CLI 的 SolarXR 设置兼容和原有 UDP/回放/SteamVR 链路。Clippy、前端类型检查、常规设置页面 lint、Vite 构建、Linux release 构建和 Windows GNU 后端交叉编译通过。实际追踪和平台交互按实机清单记录。

```sh
cd server-rust
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 tools/generate-autobone-golden.py
# 复用已生成的核心 Kotlin 参考类：
python3 tools/generate-autobone-golden.py --skip-core-build
```

重建需要 Python、JDK 17+ 和首次下载 Maven 依赖。只有 JRE 时可用 `--javac-java-modules <JDK jmods 目录>`；常规 Rust 测试读取已提交的 fixture。

整体参考统计见 [核心验证记录](rust-core-validation.zh-CN.md)。
