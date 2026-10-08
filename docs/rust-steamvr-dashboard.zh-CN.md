# SteamVR 仪表盘常用面板

范围：完整 / 航向 / 安装方向重置、实时骨架预览和节点信息。它连接现有 SlimeVR 后端，不启动第二个后端，不修改驱动注册；退出面板不会停止身体追踪。桌面程序仍负责分配、完整引导及设置。

## 使用

1. 先启动原来的 SlimeVR Rust 桌面程序 / 后端，并连接追踪器。
2. 启动 SteamVR，再启动附加包中的 `SlimeVR-Overlay.exe`。
3. 戴上头显，按手柄系统键打开 SteamVR 仪表盘，选择 **SlimeVR**。
4. 重置使用原来的校准姿势，以服务端倒计时和完成状态为准。

骨架支持前、侧、顶视角及镜像，射线按下并拖动可旋转，滚动可缩放。节点显示部位 / 自定义名称、在线状态、电量、Wi-Fi dBm 和延迟；未知值显示 `—`。

“退出面板”只退出 Overlay。SteamVR 请求退出时，Overlay 释放纹理和句柄并退出；下次启动 SteamVR 后重新打开它。SteamVR 未运行时每两秒尝试连接。

启动时优先选择 SteamVR 提供的 DXGI 显卡。如果在 SteamVR 启动前打开了面板，多显卡机器出现纹理提交错误时，请退出面板，等 SteamVR 就绪后再打开。

```powershell
.\SlimeVR-Overlay.exe --url ws://127.0.0.1:21110 --locale zh-Hans
.\SlimeVR-Overlay.exe --width-meters 2.0
.\SlimeVR-Overlay.exe --openvr-dll "D:\SlimeVR-Overlay\openvr_api.dll"
```

独立日志：`%APPDATA%\dev.slimevr.SlimeVR\logs\overlay\gui-gpui.log`。沿用现有日志分级和轮转，分目录避免两个前端竞争同一文件。

## 预览与构建

```powershell
# 纯展示，不连接后端、不执行重置。
.\SlimeVR-Overlay.exe --demo
# 真实后端及操作；Windows 同时保留 SteamVR 面板。
.\SlimeVR-Overlay.exe --preview
```

Linux 可以运行 `--preview` / `--demo` 检查界面及后端操作；OpenVR 纹理桥接目前只实现 Windows D3D11。

```sh
git submodule update --init --recursive
cargo build --manifest-path gui-gpui/Cargo.toml --release --locked --features vr-dashboard --bin slimevr-gpui-overlay
```

Windows 建议 Rust 1.92+、MSVC 和 Windows SDK。程序位于 `gui-gpui/target/release/slimevr-gpui-overlay.exe`，将固定版本 SDK 的 `bindings-provider/openvr/bin/win64/openvr_api.dll` 放在程序旁边，或用 `--openvr-dll` 指定。附加包包含它和原始 OpenVR 许可证。

GitHub Actions：打开仓库 **Actions → Build Overlay → Run workflow**，选择需要测试的分支。成功后在该次运行的 **Artifacts** 下载 `SlimeVR-Overlay-Windows-x64`，解压外层 artifact ZIP，再解压里面的附加包。无需等待 GPUI、Tauri 或后端构建。它包含 EXE、OpenVR DLL、微软运行库、启动脚本、许可证和测试说明；源码与微软运行库来源均在工作流中构建 / 校验，不依赖本地旧压缩包。Artifact 默认保留 30 天。发布合集仍可从 **SlimeVR AIO Release** 下载。

`vr-dashboard` 是可选功能，正常桌面构建不启用 GPU 输出补丁。注册的窗口以 GPU copy 更新共享 D3D11 纹理并提交给 OpenVR，不做逐帧 CPU 回读。隐藏窗口通过异步帧消息绘制；未注册的桌面窗口保持交换链显示行为。

仪表盘隐藏时停止提交，关闭骨架订阅，遥测降为 1000ms；显示时骨架为 33ms、遥测为 100ms，帧请求最高约 30 次 / 秒。`--preview` 保留桌面刷新，用于验证，不代表隐藏时的占用。

首次连接会提交一帧，供仪表盘首次选择时显示；随后按可见性更新。隐藏时宿主事件检查降至每 200ms 一次。独立锁防止重复启动，同一后端仍可同时连接桌面界面。

面板默认宽度为 **1.8 米**。顶部的“面板宽度”加减按钮以 0.1 米为步长实时调整，范围 0.5–3 米；选择保存在现有 GUI `settings.json` 的 `steamvrDashboard.widthMeters` 中，下次启动沿用。也可以启动时指定：`Start-Overlay.cmd --width-meters 2.0`。这个参数只覆盖本次启动，点击加减后会记住新值；调宽度不改变追踪器和骨架的真实坐标。

右侧使用和 GPUI 桌面主页共用的列表规则：已分配 / 未分配分组，卡片 / 表格模式，名称排序和开发模式计算节点过滤。“更多信息”打开时附加 TPS、温度、电压。每秒检查现有 GUI 设置文件的变化，改完设置无需重启 Overlay。表格列适应 VR 面板宽度，卡片使用两列布局。

骨架区域缩窄，初始缩放为 70%；可用加减按钮或滚轮缩放，“适合窗口”恢复初始比例，切换视角会保留缩放。示例模式的宽度调整不保存设置。

如果旧版只有加 `--preview` 才能正常显示，更新附加包：隐藏宿主没有应用初始尺寸，导致 SteamVR 把 DirectX 的 1×1 白色纹理拉伸成整个面板。新版在保持窗口隐藏时初始化尺寸；日志的 `overlay-texture` 会记录首次成功提交的纹理尺寸。

## 验证范围

已验证（2026-10-07）：

- 输入转换 4 项测试、Linux GUI 无桌面依赖测试 62 项通过，覆盖 DPI / Y 翻转、重复按键、取消拖动、无效滚动和重置 / 订阅策略；新增列表分组 / 排序 / 过滤、宽度持久化和 GUI 设置保存互不覆盖的检查。
- 真实 Rust 后端回环测试通过：六个模拟 UDP 追踪器、分配及 YAML 保存、三种重置和暂停。
- Linux 实际窗口确认视角切换、节点滚动、重置倒计时 / 完成提示、退出面板后后端继续运行；修正交互后静态页头和状态圆点的绘制缓存遗漏。
- 实际窗口确认面板宽度按钮、命令行覆盖后的保存、骨架缩放 / 恢复、GUI 卡片 / 表格和开发信息自动同步、设置写入中断后的恢复。VR 中实时调宽度仍需头显复测。
- Linux / Windows Clippy 检查与 Windows x64 release 构建通过；附加包审核 PE 架构、DLL 依赖、内嵌 shader 和 ZIP CRC。

没有头显运行环境，纹理显示、手柄命中、GPU 选择、清晰度和满载表现仍需 Windows / SteamVR 实测。编译通过不等于 VR 实测通过。

实机检查：打开并快速开关面板；对照桌面节点和骨架；分别做三个重置并确认点击只执行一次；断联重连不重放操作；旋转和缩放骨架、滚动节点列表；关闭 Overlay 后追踪继续；重启 SteamVR 后重新打开面板。

应用清单注册、SteamVR 自动启动及手腕 / 游戏内常驻面板留作后续功能。本版为手动启动的仪表盘附加程序。

## 重置反馈与刷新调整

通用“正在等待后端确认操作”提示已移除。按钮仍依据真实请求状态禁用，倒计时与完成提示保留。

Overlay 不重复播放桌面程序的重置音效。GPUI 桌面音效现在从通信线程收到 ResetResponse 后直接排入音频线程，独立于界面刷新；素材在音频线程启动时预解码为 PCM，保留重复回包去重和原版音效顺序。显示面板时，状态重绘合并到约 30 Hz，隐藏或暂时节流的最后一次变化不会丢失；没有新数据时不克隆整个状态。

UI 定时器若延迟超过 100 ms，日志会记录 `overlay-frame-delay`，每秒最多一条；音效队列满时记录 `audio` 警告。自动测试覆盖 UI 不读取状态时两种重置的完整音效序列、重复回包、重绘节流及隐藏恢复。实际音频输出、SteamVR 卡顿改善需要新包复测。
