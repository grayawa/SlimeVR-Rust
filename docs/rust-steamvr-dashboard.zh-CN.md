# SteamVR 仪表盘常用面板

范围：完整 / 航向 / 安装方向重置、实时骨架预览和节点信息。面板连接已运行的 SlimeVR 后端，驱动与追踪生命周期由该后端管理；桌面程序负责分配、完整引导及设置。

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

独立日志：`%APPDATA%\dev.slimevr.SlimeVR\logs\overlay\gui-gpui.log`。沿用现有日志分级和轮转，桌面与 Overlay 分别使用独立目录。

## 预览与构建

```powershell
# 使用内存示例数据展示界面。
.\SlimeVR-Overlay.exe --demo
# 桌面预览连接真实后端；Windows 同时显示 SteamVR 面板。
.\SlimeVR-Overlay.exe --preview
```

Linux 可以运行 `--preview` / `--demo` 检查界面及后端操作；OpenVR 纹理桥接目前只实现 Windows D3D11。

```sh
git submodule update --init --recursive
cargo build --manifest-path gui-gpui/Cargo.toml --release --locked --features vr-dashboard --bin slimevr-gpui-overlay
```

Windows 建议 Rust 1.92+、MSVC 和 Windows SDK。程序位于 `gui-gpui/target/release/slimevr-gpui-overlay.exe`，将固定版本 SDK 的 `bindings-provider/openvr/bin/win64/openvr_api.dll` 放在程序旁边，或用 `--openvr-dll` 指定。附加包包含它和原始 OpenVR 许可证。

GitHub Actions：打开仓库 **Actions → Build Overlay → Run workflow**，选择需要测试的分支。成功后在该次运行的 **Artifacts** 下载 `SlimeVR-Overlay-Windows-x64`，解压外层 artifact ZIP，再解压里面的附加包。独立构建完成即可下载。它包含 EXE、OpenVR DLL、微软运行库、启动脚本、许可证和测试说明；源码与微软运行库来源均在工作流中构建 / 校验，采用工作流指定的源码和下载版本。Artifact 默认保留 30 天。发布合集仍可从 **SlimeVR AIO Release** 下载。

`vr-dashboard` 是可选功能，仪表盘构建启用 GPU 输出适配。注册的窗口以 GPU copy 更新共享 D3D11 纹理并提交给 OpenVR，纹理通过 GPU 路径传递。隐藏窗口通过异步帧消息绘制；未注册的桌面窗口保持交换链显示行为。

仪表盘隐藏时停止提交，关闭骨架订阅，遥测降为 1000ms；显示时骨架为 33ms、遥测为 100ms，帧请求最高约 30 次 / 秒。`--preview` 保留桌面刷新，性能记录需注明此模式。

首次连接会提交一帧，供仪表盘首次选择时显示；随后按可见性更新。隐藏时宿主事件检查降至每 200ms 一次。独立锁防止重复启动，同一后端仍可同时连接桌面界面。

面板默认宽度为 **1.8 米**。顶部的“面板宽度”加减按钮以 0.1 米为步长实时调整，范围 0.5–3 米；选择保存在现有 GUI `settings.json` 的 `steamvrDashboard.widthMeters` 中，下次启动沿用。也可以启动时指定：`Start-Overlay.cmd --width-meters 2.0`。这个参数只覆盖本次启动，点击加减后会记住新值；面板宽度作用于显示尺寸，追踪坐标按后端数据使用。

右侧使用和 GPUI 桌面主页共用的列表规则：已分配 / 未分配分组，卡片 / 表格模式，名称排序和开发模式计算节点过滤。“更多信息”打开时附加 TPS、温度、电压。每秒检查现有 GUI 设置文件的变化，设置文件变化后自动同步。表格列适应 VR 面板宽度，卡片使用两列布局。

骨架区域缩窄，初始缩放为 70%；可用加减按钮或滚轮缩放，“适合窗口”恢复初始比例，切换视角会保留缩放。示例模式的宽度调整不保存设置。

隐藏宿主在初始化时应用客户区尺寸，首次提交的纹理尺寸记录在 `overlay-texture` 日志中。检查白屏时核对尺寸、GPU 与提交错误。

## 验证范围

验证项目：

- 输入转换与无窗口 GUI 检查覆盖 DPI / Y 翻转、重复按键、取消拖动、无效滚动和重置 / 订阅策略；列表分组 / 排序 / 过滤、宽度持久化和 GUI 与面板偏好合并的检查。
- 真实 Rust 后端回环测试通过：六个模拟 UDP 追踪器、分配及 YAML 保存、三种重置和暂停。
- Linux 实际窗口确认视角切换、节点滚动、重置倒计时 / 完成提示、退出面板后后端继续运行；页头和状态圆点随状态变化更新。
- 实际窗口确认面板宽度按钮、命令行覆盖后的保存、骨架缩放 / 恢复、GUI 卡片 / 表格和开发信息自动同步、设置写入中断后的恢复。VR 中实时调宽度仍需头显复测。
- Linux / Windows Clippy 检查与 Windows x64 release 构建通过；附加包审核 PE 架构、DLL 依赖、内嵌 shader 和 ZIP CRC。

纹理显示、手柄命中、GPU 选择、清晰度和满载表现按 Windows / SteamVR / 头显场景验收。

实机检查：打开并快速开关面板；对照桌面节点和骨架；分别做三个重置并确认点击只执行一次；断联重连恢复读取与订阅；旋转和缩放骨架、滚动节点列表；关闭 Overlay 后追踪继续；重启 SteamVR 后重新打开面板。

应用清单注册、SteamVR 自动启动及手腕 / 游戏内常驻面板留作后续功能。本版为手动启动的仪表盘附加程序。

## 重置反馈与刷新

按钮依据真实请求状态禁用，显示服务端倒计时与完成结果。

重置音效由 GPUI 桌面程序播放：通信线程收到 ResetResponse 后直接排入音频线程，素材在线程启动时预解码为 PCM，按去重后的阶段与秒数播放。Overlay 显示时状态重绘合并到约 30Hz，节流与隐藏恢复时应用最后的状态变化；有新数据时更新共享快照。

UI 定时器若延迟超过 100 ms，日志会记录 `overlay-frame-delay`，每秒最多一条；音效队列满时记录 `audio` 警告。自动测试覆盖 UI 不读取状态时两种重置的完整音效序列、重复回包、重绘节流及隐藏恢复。实际音频输出、SteamVR 卡顿改善需要新包复测。
