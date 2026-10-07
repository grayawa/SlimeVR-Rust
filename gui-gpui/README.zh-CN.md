# SlimeVR GPUI 原生前端

可选的 [SteamVR 仪表盘面板](../docs/rust-steamvr-dashboard.zh-CN.md) 提供重置、骨架预览和节点信息，使用 `vr-dashboard` 功能构建为独立附加程序。

使用 [GPUI Kit](https://gpui-kit.com/)（`longbridge/gpui-kit`，锁定 0.7.1）和 Rust 实现桌面界面，连接已有 Rust 后端。不需要 Electron、浏览器运行时或 WebView2。主页和原侧栏入口、顺序及设置分组保留。

功能对照、验证结果和实机测试步骤见 [完整性与测试说明](../docs/rust-gpui-functional-parity.zh-CN.md)。VRChat 设置警告在设置区域打开，支持返回实际来源，并保留页面滚动位置及未保存编辑。

本轮界面对齐与截图验证见 [界面对齐说明](../docs/rust-gpui-ui-alignment.zh-CN.md)。主页、分配、校准、比例、连接与设置使用原版布局和原素材作为参照，原侧栏顺序保留。设置卡片与侧栏之间、独立卡片之间均采用原版 8px 间距，外层滚动区域不重复绘制圆角。

追踪器分配复用原版人物和部位布局，设备选择使用分组卡片弹窗，支持敲击两次选择、不分配和颈部提示；窄窗口自动使用追踪点下拉选项。引导中的分配步骤共用这一界面。

test6 继续恢复佩戴选择 / 四步校准 / 手动方向轮盘、默认身高预览与分组比例、设备详情和串口控制台。覆盖手动比例前需要确认；串口关闭与重新打开状态已修正。AutoBone 完整向导和固件页仍有视觉差异；首次引导在 test8 重建，详见上述对齐说明。

`test6-handfix1` 更新随包 Rust 后端，修复手部追踪 / 手柄映射同一节点时旧设备失效消息删除新设备节点的问题。升级时完全退出旧前端和后端，避免前端继续连接旧服务。详见 [手部切换修复说明](../docs/rust-steamvr-hand-handover.zh-CN.md)。

`test7-wifi1` 补齐主页设备卡片的 Wi-Fi 信号图标与延迟，悬停查看 dBm，表格与设备详情直接显示 dBm；设备选择弹窗也复用此显示。未上报的数值保留为未知，断开连接显示灰色。保留上述手部切换修复。

`test8-onboarding1` 重新恢复原版首次引导：独立欢迎页、设备类型与 Wi-Fi / 接收器分支、连接确认、分配、佩戴、身高、用途和动作捕捉偏好。首次未完成引导时自动进入；原“引导”入口可重新打开。保留 test7 信号显示和手部切换修复。

`test9-motion1` 补齐晃动高亮：主页卡片 / 表格、设备选择菜单、引导连接列表和人物部位随旋转及线性加速度变化显示光晕，停止晃动后自动消退；断联清除旧状态。晃动不会修改分配或播放音效。保留 test8 引导及此前修复。

`test10-battery1` 将占位字符 `▰` 替换为原版电池形状和填充，恢复低电量颜色、充电闪电 / 满电勾号、断联灰色及续航显示。主页、表格、分配菜单与设备详情共用显示；悬停可查看电量、电压和已上报的续航。保留 test9 晃动高亮与此前修复。

`test11-exit1` 恢复原版“有追踪器的电源还开着”居中退出弹窗、原 Fluent 文案及上下排列的退出 / 等会按钮。Esc 与遮罩点击可取消，保留当前页面及未保存设置；修正点击穿透与关闭回调中的焦点恢复。提醒只针对仍在线且未超时的 IMU 追踪器，头显、手柄和计算节点不会误触发。确认退出停止配网并正常关闭自启动后端，连接外部服务时保留该服务。保留 test10 电量显示及此前修复。

`test12-controls1` 为设置里的数字 / 百分比加减控件增加浅背景，统一复用主题中的卡片颜色，让减号、数值和加号成为清晰的整体。保留 test11 退出弹窗和此前修复。

`test13-filtercards1` 为滤波类型及同类单选卡片统一添加主题卡片背景，选中项保留主题色边框，说明文字按实际卡片宽度换行并采用设置字号。滤波强度与航向重置过渡时长继续使用 test12 的数值条背景。保留此前修复。

`test14-components1` 将设置区域、单选卡、数值加减条和开关行抽到独立的 `ui` 组件模块，统一主题和尺寸入口，保留 test13 布局与保存逻辑。新增无需后端的 `SlimeVR-Components.exe` 预览窗口；组件边界与开发方式见 [组件库说明](../docs/rust-gpui-components.zh-CN.md)。保留此前修复。

## 功能

- 实时设备与姿态、身体部位分配、敲击识别、设备批准与遗忘、重新允许设备连接、重命名、安装方向、磁力计及详细遥测。
- 完整 / 航向 / 安装方向重置，脚部与手指校准，暂停，去重倒计时音效。
- 手动身体比例、分组比例、按身高重建比例、导入 / 导出、AutoBone 录制 / 停止 / 取消 / 处理 / 保存 / 应用，以及身高校准和 Stay Aligned。
- SteamVR 输出、跟踪机制、FK、手势、OSC 路由、VRChat OSC、VMC、VRM 导入、快捷键、检查清单和高级设置。
- 串口、Wi-Fi 配网、官方 / 自定义 OTA、固件构建服务与串口刷写，分批发布和电量检查。
- 原生三维骨架与追踪器模型、视角 / 镜像 / 拖动 / 缩放、BVH 录制和文件选择。
- 原版 29 种语言及 Fluent 回退、`override.ftl`、主题、嵌入字体、显示和音效偏好、引导。
- Windows 托盘、单实例唤起、后端启动 / 优雅退出、日志级别和轮转、目录打开、剪贴板与 Discord presence。

后端继续读写 `vrconfig.yml` / `.yaml`。GUI 偏好兼容现有 `settings.json` 中的 `config.json`，保留未知字段；这是既有 GUI 偏好，不是另建后端 JSON 配置。

## 使用

Windows 解压完整包后双击 `SlimeVR.exe`。旁边有 `slimevr-server.exe` 时自动启动后端；端口已有服务时连接已有后端。默认地址 `ws://127.0.0.1:21110`。

配置：`%APPDATA%\dev.slimevr.SlimeVR\vrconfig.yml`，也识别 `.yaml`。日志：`%APPDATA%\dev.slimevr.SlimeVR\logs\gui-gpui.log`。默认 info / warn / error；10 MiB 轮转，保留 4 个历史文件。只读诊断工具的输出可能包含设备信息。

```powershell
.\SlimeVR.exe --attach
.\SlimeVR.exe --backend "D:\SlimeVR\slimevr-server.exe" --config "D:\SlimeVR\vrconfig.yml"
.\SlimeVR.exe --log-level debug
.\SlimeVR.exe --url ws://127.0.0.1:21110 --locale zh-Hans
```

退出自己启动的后端时，关闭其 stdin，等待后端保存录制并正常退出；连接别人启动的服务时不结束该服务。断联仅恢复订阅和读取，重置 / 录制 / 刷写不自动重发。

## 构建与检查

Rust 1.92+，Windows 本机建议 MSVC、Windows SDK 和 CMake，依 [GPUI Kit 安装说明](https://gpui-kit.com/docs/installation) 准备。

```powershell
cargo test --manifest-path gui-gpui/Cargo.toml --no-default-features --locked
cargo clippy --manifest-path gui-gpui/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path gui-gpui/Cargo.toml --release --locked
```

前端位于 `gui-gpui/target/release/slimevr-gpui.exe`。另有 `slimevr-gpui-probe` 只读诊断程序。Linux 需 GPUI 的图形和 ALSA 开发依赖。

Windows GNU 交叉构建使用源码内 `vendor/gpui-pre-windows` 的 `runtime-shaders` 补丁：把两份 HLSL 和共享 include 嵌入程序，通过系统 D3DCompiler_47 在内存编译优化着色器。Release 不开启图形调试断言，也不读取构建机器上的源文件路径。打包会检查 PE 导入表并拒绝旧的 `D3DCompileFromFile` 加载方式。Windows MSVC 工作流会构建后端、前端和诊断工具并运行 loopback 检查，尚未远程执行。

设置页结构由原 React TSX 提取，保留标题、说明、控件顺序、选项与字段映射；原翻译继续使用 Fluent。检查清单遵循原前端状态计算，本次忽略只在 GUI 会话内生效，设置中的开关永久保存到后端配置。驱动状态提示只放在 SteamVR 设置中。

RPC 适配器使用现有 SolarXR Rust 绑定，不定义新网络协议。重新生成（设置提取需先安装仓库 Node 依赖）：

```sh
python gui-gpui/scripts/generate-rpc.py
python gui-gpui/scripts/generate-catalog.py
python gui-gpui/scripts/generate-i18n.py
python gui-gpui/scripts/generate-meshes.py
python gui-gpui/scripts/generate-ui-assets.py
node gui-gpui/scripts/generate-settings-layout.cjs
cargo fmt --manifest-path gui-gpui/Cargo.toml
```

字体使用随源码提交的 TTF；原 WOFF 转换与来源见 `assets/fonts/SOURCES.md`。发布包只装载编译程序、原版驱动和运行依赖，不包含用户配置或日志。
