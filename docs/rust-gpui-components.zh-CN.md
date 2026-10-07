# GPUI 内部组件库

2026-10-07，第一轮 `GPUI-test14-components1`。

组件位于 `gui-gpui/src/ui/`，在原生 `desktop` feature 下启用。仍使用 GPUI Kit 0.7.1 的按钮、开关、焦点和键盘交互，不引入新的 UI 运行时。第一轮整理设置页，保留 test13 的布局、文案、主题颜色、参数范围及保存行为。

## 分层与接口

```text
ui/
├── theme.rs                  主题初始化、实时颜色角色、设置控件尺寸
└── components/
    ├── card.rs               card 基础表面、SettingsPane 设置区域
    ├── choice_card.rs        ChoiceCard 单选卡片
    ├── number_selector.rs    NumberSelector 数值加减条
    └── setting_row.rs        SwitchRow 开关行
```

`Surface::Panel` 对应页面面板背景，`Surface::Control` 对应控件卡片背景。颜色在组件构建时从当前主题读取，不在组件中缓存；圆角、内边距、间距和最低高度集中在 `theme.rs`。原 `apply_theme` 移到同一模块，软件界面与组件预览共用，主题偏好和高对比度选项仍由软件页面管理。

组件接收已翻译的标题、说明、显示值及原生交互控件。`build(cx)` 返回 GPUI 元素，页面可继续绑定回调或组合布局。组件不依赖 `SlimeView`、SolarXR、配置路径、网络、引导或页面导航。

| 组件 | 组件负责 | 页面负责 |
|---|---|---|
| SettingsPane / card | 背景、圆角、区域布局、标题及图标容器 | 文案翻译、图标素材和页面内容 |
| ChoiceCard | 卡片布局、选中边框、禁用样式、实际宽度换行和指定字号 | 选中值、选项意义、点击后的状态修改 |
| NumberSelector | 标签、数值条及两端按钮排列 | 单位格式、整数/浮点转换、步长、上下限、禁用按钮和修改回调 |
| SwitchRow | 标签、开关排列、可访问名称、整行禁用 | 开关当前值、确认流程和修改回调 |

例如，页面绑定单选卡的选择事件：

```rust
ChoiceCard::new("filter-smoothing", translated_title)
    .description(Some(translated_description.into()))
    .checked(current == smoothing)
    .disabled(disabled)
    .text_size(px(preferred_text_size))
    .build(cx)
    .on_click(cx.listener(move |this, _, _, cx| {
        this.layout_set(&node, json!(smoothing), cx)
    }))
```

设置描述 JSON 继续决定控件类型、字段、范围、条件和分组。`settings_layout_ui.rs` 将这些业务数据交给组件；滤波、FK、OSC 及界面偏好中复用同类控件的设置会使用同一组件。`Draft`、RPC、保存/放弃、断联处理以及特殊开关确认流程保持原实现。

## 独立组件预览

开发环境：

```sh
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components -- --locale en --theme light --text-size 16
```

Windows 包中的 `SlimeVR-Components.exe` 使用同一组件库。可切换 Slime / Light / Green 主题、12 / 16 / 20px 字号，选择滤波卡片，调整百分比和秒数，操作开关并查看禁用状态与长中英文说明。窗口缩窄后单选卡纵向排列，内容可滚动。

预览只使用内存中的示例值，不启动或连接后端，也不读取或保存 `settings.json`、`vrconfig.yml`、个人日志和软件偏好。它是开发检查入口；正常使用仍运行 `SlimeVR.exe`。预览内的参数范围用于展示控件，正式页面的范围仍来自原设置描述。

## 本轮验证

- 现有前端通信、状态、配置和翻译测试重新运行：57 项通过，1 项外部后端联调测试按默认配置忽略。本轮不新增重复样式实现的单元测试。
- Linux 与 Windows GNU 的全部 target Clippy（`-D warnings`）、格式检查和两平台构建通过。正式 GUI 与独立预览的 Windows shader 嵌入检查通过。
- Linux 原生窗口连接真实 Rust 后端与六台模拟 UDP 设备。默认设置内容区域与 test13 逐像素一致；检查单选、数值加减、开关、跨页草稿、保存和放弃。保存后核对 YAML 中滤波类型、强度及 `resetsConfig` 的过渡时长和安装校准保存选项；未保存的操作不写回 YAML。
- 正式窗口检查 Light 与恢复 Slime 主题、800 × 560 窗口；主题恢复后画面与切换前一致。
- 独立预览检查三种主题、12 / 16 / 20px 字号、560 × 650 窗口与滚动、长中英文、控件交互和禁用状态。主题恢复后画面一致，预览交互不修改临时 GUI 偏好或 YAML。
- Windows 包仍需 Windows 11 实机确认；本轮没有在 Windows 上执行 GUI。

## 后续边界

第一轮未迁移弹窗、设备卡片、保存栏、输入框或下拉菜单；少数独立页面和旧备用路径中的手写控件也仍在原文件中。后续先整理弹窗交互，再提取追踪器显示，最后拆分页面状态。组件模块稳定后可独立为 crate，目前不增加跨 crate 接口成本。

组件化减少重复样式与维护工作；本轮没有进行 Windows 内存或 CPU 性能对比。
