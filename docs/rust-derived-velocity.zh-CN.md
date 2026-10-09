# Rust 派生速度算法

日期：2026-10-04。参考提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

派生线速度参考 `Tracker.updateDerivedVelocity()`，通过核心、SteamVR Protobuf 和 SolarXR 设置提供，配置保存在原版 YAML。速度单位为米 / 秒。

## 使用

前端“设置 → 常规 → 速度设置”可开启“发送派生速度”；直接编辑原版配置也可以：

```yaml
velocityConfig:
  sendDerivedVelocity: true
```

缺省为 `false`，沿用原版选择。启用后，每个解算 tick 独立计算 11 个计算追踪器的速度；SteamVR 输出的 Position 消息按需填入 `vx`、`vy`、`vz`。关闭时省略三个字段，驱动按原协议处理为没有速度输入。前端能力通知增加 `derived_velocity`，设置读取、修改、恢复默认值及保存沿用已有 SolarXR 消息。

`PoseConfig.send_derived_velocity` 为核心开关。`PoseSnapshot.computed_velocities` 按计算追踪器名称返回有效速度；没有有效数据时省略该 JSON 字段。journal 使用原有配置、控制和 tick 记录，可以重建相同速度。

## 计算与时序

`v = (当前位置 - 上次位置) / dt`。首次观察只建立基线；有效间隔为 **100 微秒至 250 毫秒，包含端点**。零间隔、倒退时钟或过长/过短间隔重新建立位置基线，该次速度为缺失。除法使用双精度秒数，再转换为 f32，与原版一致。

独立 `DerivedVelocity` 使用显式微秒时钟；现有 PoseEngine / journal 使用毫秒时钟，所以解算循环可表示的有效间隔是 1–250 ms。速度计算频率取决于解算 tick，与 UDP 样本频率及驱动消息发送频率分开。

集成有以下明确选择：

- 原版 HumanSkeleton 在更新 FK 计算追踪器时计算速度，之后才执行 LegTweaks。Rust 从 LegTweaks 后的最终 `skeleton.computed` 计算速度，使输出速度与发送给驱动的位置一致；两种实现分别按其阶段顺序测量输出。
- 仅在有外部世界位置锚点且未暂停时计算；SteamVR 世界速度使用外部锚点下的最终位置。丢失锚点或暂停会清除历史，恢复后的首帧只建立基线。
- Full/Yaw/Mounting reset、外部来源移除、设备会话重启、绑定/安装修正变更、骨长或滤波/腿部/对齐/定位参数变化会清除历史，使下一帧从当前有效位置重建基线。临时腿部参数和测眼高后的骨长更新同样处理。
- 非有限位置或溢出结果标记为缺失。计算结果直接作为线速度输出，有效零速度发送为零。

该速度由输出位置差分产生，输入为最终位置与显式时间。

## 验证

参考生成器提取仓库真实 Kotlin `Tracker.updateDerivedVelocity()` 方法，仅将单调时钟替换为 `TestTimeSource`。两个参考序列共 18 次观察覆盖 99/100 微秒、250000/250001 微秒、零 dt、关闭/重新开启、位置丢失和历史重置；Rust 输出按现有 `3e-6 × (1 + abs(expected))` 分量容差比较，数据缺失必须一致。

核心测试覆盖最终脚部位置、平移、无世界锚点、复位/暂停/重连、配置变更、异常位置和数值溢出。SteamVR 测试验证可选速度字段及失效行为；真实 CLI + socket + RPC 测试启用速度，并逐个比较在线发布帧和 journal 回放帧。配置测试覆盖缺省、字段校验、保存/重载和未知字段保留；前端绑定测试覆盖能力、读取和关闭后的 YAML 保存。

```sh
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
python3 server-rust/tools/generate-core-golden.py
pnpm --dir gui test:backend
```

两个 Kotlin 速度参考序列与核心 / 输出 / 配置检查覆盖上述窗口与状态边界。真实 SteamVR 和六点追踪器的速度表现按实机记录验收。

实现：[velocity.rs](../server-rust/crates/slimevr-core/src/velocity.rs)、[pose.rs](../server-rust/crates/slimevr-core/src/pose.rs)、[SteamVR 桥](../server-rust/crates/slimevr-server/src/steamvr/mod.rs)。
