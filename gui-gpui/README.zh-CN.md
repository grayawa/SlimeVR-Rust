# SlimeVR GPUI 原生前端

桌面界面采用 [GPUI Kit](https://gpui-kit.com/) 0.7.1 和 Rust，通过 SolarXR 连接后端。主侧栏、设置分组、Fluent 翻译和素材以原 React 界面为参照。可选 [SteamVR 仪表盘](../docs/rust-steamvr-dashboard.zh-CN.md) 提供重置、骨架和节点信息。

## 页面与功能

- 主页：设备分组、卡片 / 表格、状态、电量、Wi-Fi 信号与延迟、晃动高亮、重置、暂停、骨架和 BVH。
- 分配：原人物与身体部位布局、设备分组弹窗、两次敲击选择、取消分配、颈部提示及窄窗口选项。
- 佩戴与比例：佩戴选择、四步校准、手动方向轮盘、分组比例、身高预览、导入 / 导出和 AutoBone 向导。
- 设备：批准、遗忘、重新允许连接、命名、安装方向、磁力计、遥测和串口控制台。
- 设置：SteamVR、滤波、FK / 约束 / 腿部修正、手势、StayAligned、快捷键、OSC 路由、VRChat、VMC / VRM 与检查清单。
- 连接与固件：原版首次引导、Wi-Fi / 接收器分支、串口配网、官方 / 自定义 OTA、固件构建服务与串口刷写。
- 桌面：29 种语言、英文回退、`override.ftl`、主题、字体、字号、声音、托盘、单实例、文件对话框、日志和 Discord Presence。

VRChat 设置警告在设置区域打开，返回时恢复来源页、滚动位置与草稿。设置卡片之间采用 8px 间距，控件从当前主题读取背景和选中颜色。详细布局与交互见 [界面对齐](../docs/rust-gpui-ui-alignment.zh-CN.md)，功能覆盖与验收见 [功能与测试](../docs/rust-gpui-functional-parity.zh-CN.md)。

## 使用与数据

Windows 完整解压包中双击 `SlimeVR.exe`。同目录的 `slimevr-server.exe` 用于自动启动后端；本机端口已有服务时连接该服务。默认地址为 `ws://127.0.0.1:21110`。

Windows 后端使用 `%APPDATA%\dev.slimevr.SlimeVR\vrconfig.yml`，也识别 `.yaml`。GUI 偏好沿用 `settings.json` 中的 `config.json`，保存时保留未知字段。日志位于 `%APPDATA%\dev.slimevr.SlimeVR\logs\gui-gpui.log`，默认级别为 info，约 10MiB 轮转并保留四份历史文件。

```powershell
.\SlimeVR.exe --attach
.\SlimeVR.exe --backend "D:\SlimeVR\slimevr-server.exe" --config "D:\SlimeVR\vrconfig.yml"
.\SlimeVR.exe --log-level debug
.\SlimeVR.exe --url ws://127.0.0.1:21110 --locale zh-Hans
```

自己启动的后端在退出时通过 stdin EOF 完成录制并关闭；外部服务由原启动方式管理。重连恢复读取与订阅，用户操作按当前会话确认。退出提醒针对在线且未超时的 IMU 追踪器，Esc 或遮罩点击可返回当前页面并保留草稿。

Linux x64 完整包以 Ubuntu 24.04 为构建基线。下载 `SlimeVR-GPUI-Linux-x64` artifact，解开外层 ZIP 和里面的 `tar.gz`，在固定目录执行 `./Start-SlimeVR.sh`。程序与同目录的后端、驱动和 OpenVR helper 配套使用；已有服务可以用 `--attach` 连接。

Linux 配置与 GUI 偏好位于 `${XDG_CONFIG_HOME:-$HOME/.config}/dev.slimevr.SlimeVR/`，日志在其中的 `logs/gui-gpui.log`。界面通过 Vulkan 在 X11 / Wayland 上渲染。托盘使用桌面会话的 StatusNotifier/D-Bus 服务；GNOME 可启用 AppIndicator 扩展。托盘的显示、最小化和退出菜单沿用 GUI 语言。启用托盘且桌面托盘可用时，关闭窗口会最小化并保持后端运行；其余情况下按退出流程回收自己启动的后端。

随包的 `69-slimevr-devices.rules` 为串口和 SlimeNRF HID 设备提供当前桌面用户的访问规则。出现设备权限提示时，从解压目录执行以下命令，然后重新插入设备：

```sh
sudo install -m 644 69-slimevr-devices.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
```

Linux 自动验证覆盖 X11 软件 Vulkan 渲染、托盘 D-Bus 注册与菜单、语言更新、图标像素格式、真实后端回环和分发文件校验。Wayland 桌面、物理音频设备、SteamVR / VRChat 与无线追踪器按 [实机清单](../docs/rust-unified-hardware-test.zh-CN.md) 验收。SteamVR 仪表盘的纹理提交使用 Windows D3D11；Linux 的 Overlay 可用于桌面预览。

`SlimeVR-Components.exe` 使用内存示例值展示组件。正常使用运行 `SlimeVR.exe`。独立的 `slimevr-ui` crate 位于 `ui/`，由桌面、Overlay 和预览共用；组件接口见 [组件库](../docs/rust-gpui-components.zh-CN.md)。

## 构建与生成

使用 Rust 1.92+；Windows 建议 MSVC、Visual Studio C++ Build Tools、Windows SDK 和 CMake，系统依赖见 [GPUI Kit 安装说明](https://gpui-kit.com/docs/installation)。Ubuntu 24.04 的 Linux 构建依赖可通过以下命令安装：

```sh
sudo apt-get install g++-14 cmake ninja-build pkg-config libfontconfig-dev libasound2-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libwayland-dev libvulkan-dev libudev-dev libssl-dev
```

运行时需要系统 Vulkan 驱动（例如发行版的 Mesa 或厂商驱动）、ALSA、Fontconfig、X11 / Wayland 和桌面 D-Bus 会话。

```sh
cargo test --manifest-path gui-gpui/Cargo.toml --no-default-features --locked
cargo clippy --manifest-path gui-gpui/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo build --manifest-path gui-gpui/Cargo.toml --release --locked
```

Windows 主程序位于 `gui-gpui/target/release/slimevr-gpui.exe`，`slimevr-gpui-probe` 提供只读诊断。Windows 渲染器通过 `runtime-shaders` 嵌入 HLSL 并使用 D3DCompiler_47 在内存编译。打包检查 shader、PE / DLL、许可证、ZIP CRC 与 SHA-256。

RPC 适配依赖现有 SolarXR Rust 绑定。设置字段与布局从 React TSX 提取，翻译使用 Fluent。重新生成时从仓库根目录执行：

```sh
python gui-gpui/scripts/generate-rpc.py
python gui-gpui/scripts/generate-catalog.py
python gui-gpui/scripts/generate-i18n.py
python gui-gpui/scripts/generate-meshes.py
python gui-gpui/scripts/generate-ui-assets.py
node gui-gpui/scripts/generate-settings-layout.cjs
cargo fmt --manifest-path gui-gpui/Cargo.toml -p slimevr-gpui -p slimevr-ui
```

设置布局生成需要已安装仓库 Node 依赖。字体来源与转换记录见 [SOURCES.md](assets/fonts/SOURCES.md)，结构与开发边界见 [前端架构](../docs/rust-gpui-frontend-plan.zh-CN.md)。

## Actions 分发

打开 **Actions → Build GPUI → Run workflow**，选择分支，下载成功运行的 `SlimeVR-GPUI-Windows-x64` 或 `SlimeVR-GPUI-Linux-x64`。两平台并行构建，分别上传 artifact。发布合集使用 **SlimeVR AIO Release**。触发方式、下载与验证范围见 [CI](../docs/rust-ci.zh-CN.md)。

解压包提供原生前端、后端、驱动、OpenVR helper、运行依赖和许可 / 源码版本说明。用户配置与日志保存在应用数据目录。GPUI 界面由原生渲染器绘制，Tauri 在 Windows 使用 WebView2，在 Linux 使用 WebKitGTK。
