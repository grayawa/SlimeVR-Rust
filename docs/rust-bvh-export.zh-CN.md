# Rust BVH 导出

日期：2026-10-03。参考上游提交：`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

Rust 后端已支持现有 Web / Electron / Tauri 界面的 BVH 录制入口。启动和连接方式见 [前后端联调](rust-frontend-integration.zh-CN.md)。点击骨架预览下方的录制图标开始，再次点击停止；停止后显示实际保存路径和帧数。

## 保存位置

- 桌面端使用已有保存对话框，或界面设置中的 BVH 目录。目录存在时自动选取第一个未占用的 `BVH-RecordingN.bvh`。
- Web 端未指定路径时，保存到当前 `vrconfig.yml` / `.yaml` 所在目录的 `recordings/`，该默认目录自动创建。可用 `--config <路径>` 指定配置及相应录制目录；未指定时使用原版配置默认位置。
- 文件位于**运行后端的电脑**。浏览器会显示路径，不自动下载录像；连接远程后端时也是远程电脑的文件。
- 显式指定文件必须使用 `.bvh` 扩展名，父目录必须存在。已有文件不会覆盖，选择新文件名或目录即可继续录制。

BVH 与 `--record xxx.jsonl` 的协议／算法回放日志各自独立，AutoBone 的训练录制也使用独立状态。

## 对照原版的导出规则

主要参考：

- `server/core/src/main/java/dev/slimevr/posestreamer/BVHFileStream.kt`
- 同目录 `BVHSettings.kt`、`BVHRecorder.kt`、`TickPoseStreamer.kt`
- `server/core/src/main/java/dev/slimevr/util/TickReducer.kt`
- `server/core/src/main/java/dev/slimevr/tracking/processor/skeleton/HumanSkeleton.kt`

| 项目 | Rust 行为与原版对应 |
| --- | --- |
| 单位 | 使用原版默认 `BVHSettings.BLENDER`：位置和偏移均为米，缩放均为 1。`DEFAULT` 的 100 倍缩放不是原版 GUI 录制器实际使用的默认值 |
| 根节点 | `ROOT HIP`，以 HIP 的尾部世界位置为根平移 |
| 层级 | 从 HIP 反向遍历腰、胸、颈、头，再按原顺序遍历分支；保持反向偏移、零偏移、跟踪器、手臂和手指节点及 End Site |
| 节点数量 | 普通模式 65 个通道节点；单侧控制器 46 个，双侧控制器 27 个。原版控制器手臂是独立的树，HIP 导出只遍历其所在连通分量 |
| 通道 | 根节点为 `Xposition Yposition Zposition Zrotation Xrotation Yrotation`；其他节点为 `Zrotation Xrotation Yrotation` |
| 旋转 | 严格保留 `bone.rotationOffset.inv() * invertParentRot * bone.globalRotation` 的乘法顺序，再使用 ktmath 对应的 ZXY 欧拉分解，包括 ±90° 分支，转为角度 |
| 采样 | `Frame Time: 0.01`，沿用 TickReducer 的相位补偿；一个核心 tick 最多写一帧，不插值、不补发积压帧。调度变慢时实际采样频率可能低于 100 Hz，文件时间步仍为 0.01 s，与原方法一致 |
| 数据来源 | 约束后的 FK 骨骼，和原导出器一样；不替换成 LegTweaks 后处理的计算追踪器位置 |
| 暂停 | 仍按采样时序导出当前姿态 |
| 文件写入 | 使用缓冲流持续写帧，内存不保留整段录像；帧数预留 19 字符，停止时 seek 回填 |

这里保持原版结构，没有额外改成人形重定向骨架。Blender 中需要按米尺度使用，通道节点中包含跟踪器骨骼。

## 生命周期与接口

继续使用原 SolarXR `RecordBVHRequest`、`RecordBVHStatusRequest`、`RecordBVHStatus`。查询和操作响应保留事务编号；录制状态向全部连接广播。重复开始不会重新打开文件，重复停止不会重复生成录像。连接期间开启 `bvh` 能力，恢复现有界面的录制入口。

停止后附加文本通知：

```json
{"type":"backend_file_saved","path":"/absolute/path/recording.bvh","frames":100}
```

前端解析后展示保存提示，失败则使用现有错误弹窗。二进制 SolarXR 表没有改动，Java 后端仍走原有消息流程。

录制开始时固定导出骨架。录制期间导出分量的骨长、偏移或层级变化，自动停止并补齐已写帧数，提示重新开始。这个检查避免旧头部定义与新通道／骨长混用；HMD 首次出现或切换控制器模式也可能触发它。因此先接好设备、完成比例校准，再录制。

手动服务通过 Ctrl-C 或 `--run-for` 正常退出时收尾。Tauri 启动自有 Rust 后端时传入 `--shutdown-on-stdin-eof` 并保持父进程输入管道；关窗时关闭管道，让后端正常完成 BVH 和 journal 收尾，最多等 3 秒后才兜底终止。强制结束进程或断电不保证文件完整。

相较原版，当前实现增加了默认 Web 保存目录、已有文件保护、骨架变化自动收尾，以及前端保存位置提示。

## 验证与复现

`generate-bvh-golden.py` 实际运行未修改的 Kotlin BVHFileStream、BVHSettings、PoseDataStream、TickReducer 和缓存的原 ktmath；HumanSkeleton 的两个装配方法原样提取，独立组装骨架并检查 Rust 的父子关系和子节点顺序。Bone 适配器提供同一组已解算快照，因此这组对照验证的是**导出和拓扑**，FK 算法本身另见 [核心验证](rust-core-validation.zh-CN.md)。参考源码 SHA-256 保存在 fixture 中。

6 段对照录像共 48 帧，覆盖普通骨架、约束、单侧／双侧控制器、无 HMD、旋转符号翻转以及 ±90° 奇异点。层级与通道顺序一致，偏移容差为 `4e-6 m`，根位置容差为 `1e-6 m`；重建旋转后的最大差异约 `0.00001763°`，测试阈值为 `0.005°`。奇异点允许等价的欧拉角表示。4 组真实 Kotlin TickReducer 采样序列逐次匹配。

```sh
cd server-rust
cargo test --workspace --locked
cargo test --locked --test bvh -- --nocapture
cargo clippy --workspace --all-targets --locked -- -D warnings

# 重新生成参考，需要先由原核心参考工具准备 Kotlin / ktmath 缓存。
python3 tools/generate-core-golden.py
python3 tools/generate-bvh-golden.py
```

GUI 的 `pnpm --dir gui test:backend` 验证真实服务、前端 TypeScript 绑定、状态同步、暂停、默认／显式路径、不覆盖、骨长变化、Ctrl-C 与父管道 EOF 收尾；父管道关闭后的 journal 仍可正常回放。Rust 共 76 项测试，前端协议／集成 6 项、桌面适配 5 项。浏览器实际点击开始／停止并保存 50 帧、展示保存提示，页面无运行错误。前端类型检查、构建、改动文件 ESLint、Rust Clippy 和 Windows GNU 后端交叉编译通过。

未做 Blender 实际导入或真实硬件录制验收，Windows 后端本次只交叉编译。Linux Tauri 原生宿主可编译并通过测试；BVH 原生对话框的完整交互仍需桌面验收。

## 实现入口

- `server-rust/crates/slimevr-server/src/bvh.rs`：骨架重排、偏移、旋转、采样及文件生命周期。
- `server-rust/crates/slimevr-core/src/skeleton.rs`：快照增加实际父子关系与顺序，包含动态控制器骨架。
- `server-rust/crates/slimevr-core/src/math.rs`：ZXY 欧拉角转换。
- `server-rust/crates/slimevr-server/src/api/mod.rs`：录制 RPC、广播与保存通知。
- `server-rust/crates/slimevr-server/src/runtime.rs`、`gui/src-tauri/src/server.rs`：正常退出与父子进程收尾。
- `gui/src/hooks/bvh.ts`、`gui/src/components/BVHButton.tsx`、`gui/src/components/BVHSaved.tsx`：现有入口与保存结果。
- `server-rust/tools/generate-bvh-golden.py`、`server-rust/crates/slimevr-server/tests/bvh.rs`、`gui/tests/backend.test.ts`：原版对照与集成验证。
