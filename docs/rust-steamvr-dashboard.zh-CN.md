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
.\SlimeVR-Overlay.exe --width-meters 1.3
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

GitHub Actions：打开仓库 **Actions → SteamVR dashboard Windows bundle → Run workflow**。成功后在该次运行的 **Artifacts** 下载 `SlimeVR-Overlay-Windows-x64`，解压外层 artifact ZIP，再解压里面的附加包。它包含 EXE、OpenVR DLL、微软运行库、启动脚本、许可证和测试说明；源码与微软运行库来源均在工作流中构建 / 校验，不依赖本地旧压缩包。Artifact 默认保留 30 天。

`vr-dashboard` 是可选功能，正常桌面构建不启用 GPU 输出补丁。注册的窗口以 GPU copy 更新共享 D3D11 纹理并提交给 OpenVR，不做逐帧 CPU 回读。隐藏窗口通过异步帧消息绘制；未注册的桌面窗口保持交换链显示行为。

仪表盘隐藏时停止提交，关闭骨架订阅，遥测降为 1000ms；显示时骨架为 33ms、遥测为 100ms，帧请求最高约 30 次 / 秒。`--preview` 保留桌面刷新，用于验证，不代表隐藏时的占用。

首次连接会提交一帧，供仪表盘首次选择时显示；随后按可见性更新。隐藏时宿主事件检查降至每 200ms 一次。独立锁防止重复启动，同一后端仍可同时连接桌面界面。

## 验证范围

已验证（2026-10-07）：

- 输入转换 4 项测试、GUI 无桌面依赖测试 59 项通过，覆盖 DPI / Y 翻转、重复按键、取消拖动、无效滚动和重置 / 订阅策略。
- 真实 Rust 后端回环测试通过：六个模拟 UDP 追踪器、分配及 YAML 保存、三种重置和暂停。
- Linux 实际窗口确认视角切换、节点滚动、重置倒计时 / 完成提示、退出面板后后端继续运行；修正交互后静态页头和状态圆点的绘制缓存遗漏。
- Linux / Windows Clippy 检查与 Windows x64 release 构建通过；附加包审核 PE 架构、DLL 依赖、内嵌 shader 和 ZIP CRC。

没有头显运行环境，纹理显示、手柄命中、GPU 选择、清晰度和满载表现仍需 Windows / SteamVR 实测。编译通过不等于 VR 实测通过。

实机检查：打开并快速开关面板；对照桌面节点和骨架；分别做三个重置并确认点击只执行一次；断联重连不重放操作；旋转和缩放骨架、滚动节点列表；关闭 Overlay 后追踪继续；重启 SteamVR 后重新打开面板。

应用清单注册、SteamVR 自动启动及手腕 / 游戏内常驻面板留作后续功能。本版为手动启动的仪表盘附加程序。
