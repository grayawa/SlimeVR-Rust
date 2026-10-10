# GPUI 组件库

`slimevr-ui` 是位于 `gui-gpui/ui/` 的独立 Rust crate，直接依赖 GPUI Kit 0.7.1，提供按钮、开关、焦点和键盘交互的组合组件。桌面主程序、SteamVR Overlay 与组件预览通过应用的 `desktop` feature 引入该 crate。设置布局、文案、参数范围与保存规则由调用页面提供。

## 分层与接口

```text
gui-gpui/
├── Cargo.toml                workspace、应用依赖与 Windows 渲染器补丁
├── src/theme.rs              SlimeVR 主题预设
└── ui/
    ├── Cargo.toml            slimevr-ui；依赖 gpui-kit
    └── src/
        ├── lib.rs            组件与主题角色导出
        ├── theme.rs          当前主题颜色角色与控件尺寸
        └── components/
            ├── card.rs               card 基础表面、SettingsPane 设置区域
            ├── choice_card.rs        ChoiceCard 单选卡片
            ├── number_selector.rs    NumberSelector 数值加减条
            └── setting_row.rs        SwitchRow 开关行
```

`Surface::Panel` 对应页面面板背景，`Surface::Control` 对应控件卡片背景。颜色在组件构建时从当前主题读取；圆角、内边距、间距和最低高度集中在 `theme.rs`。`slimevr_gpui::theme::apply_theme` 提供应用主题预设，由桌面、Overlay 与组件预览调用。消费应用通过 GPUI Kit 的主题接口配置颜色；组件构建时读取当前主题。主题偏好和高对比度选项由应用页面管理。

组件接收已翻译的标题、说明、显示值及原生交互控件。`build(cx)` 返回 GPUI 元素，页面可继续绑定回调或组合布局。组件通过受控输入和页面绑定的回调组合，业务依赖由页面持有。

| 组件                | 组件负责                                             | 页面负责                                                  |
| ------------------- | ---------------------------------------------------- | --------------------------------------------------------- |
| SettingsPane / card | 背景、圆角、区域布局、标题及图标容器                 | 文案翻译、图标素材和页面内容                              |
| ChoiceCard          | 卡片布局、选中边框、禁用样式、实际宽度换行和指定字号 | 选中值、选项意义、点击后的状态修改                        |
| NumberSelector      | 标签、数值条及两端按钮排列                           | 单位格式、整数/浮点转换、步长、上下限、禁用按钮和修改回调 |
| SwitchRow           | 标签、开关排列、可访问名称、整行禁用                 | 开关当前值、确认流程和修改回调                            |

例如，页面绑定单选卡的选择事件：

```rust
use slimevr_ui::components::ChoiceCard;

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

## crate 与构建边界

`gui-gpui/Cargo.toml` 管理应用和 `ui` 两个 workspace 成员，默认成员为应用。组件依赖通过 `desktop` feature 启用；通信与状态检查使用应用的 `--no-default-features`。应用与组件共享 `gui-gpui/Cargo.lock`、GPUI Kit 版本及根 manifest 的 Windows 渲染器补丁。OpenVR runtime 和 vendor 使用各自 manifest。

单独检查组件及整个原生 workspace：

```sh
cargo check --manifest-path gui-gpui/ui/Cargo.toml --locked
cargo clippy --manifest-path gui-gpui/Cargo.toml --workspace --all-targets --features vr-dashboard --locked -- -D warnings
cargo fmt --manifest-path gui-gpui/Cargo.toml -p slimevr-gpui -p slimevr-ui --check
```

其他 GPUI 应用可以通过 path 依赖引用 `gui-gpui/ui`，使用 `slimevr_ui::components` 和 `slimevr_ui::theme`。组件接收已翻译的文案与受控状态，调用方负责 GPUI 初始化、主题、字体与事件处理。独立接入示例见 [crate README](../gui-gpui/ui/README.md)。

## 独立组件预览

开发环境：

```sh
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components -- --locale en --theme light --text-size 16
```

Windows 包中的 `SlimeVR-Components.exe` 使用同一组件库。可切换 Slime / Light / Green 主题、12 / 16 / 20px 字号，选择滤波卡片，调整百分比和秒数，操作开关并查看禁用状态与长中英文说明。窗口缩窄后单选卡纵向排列，内容可滚动。

预览的数据、设置和交互全部保存在示例窗口的内存中。它是开发检查入口；正常使用仍运行 `SlimeVR.exe`。预览内的参数范围用于展示控件，正式页面的范围仍来自原设置描述。

## 组件检查

- 前端通信、状态、配置和翻译由现有自动检查覆盖，样式通过组件预览与原生窗口检查。
- crate 拆分的检查覆盖组件独立编译、workspace Clippy、桌面 / Overlay / 预览的依赖接入、共享锁文件及格式。Windows 分发构建执行 workspace Clippy 与 shader 嵌入检查。
- Linux 原生窗口连接真实 Rust 后端与六台模拟 UDP 设备。内容区域按 React 设置结构对照；检查单选、数值加减、开关、跨页草稿、保存和放弃。保存后核对 YAML 中滤波类型、强度及 `resetsConfig` 的过渡时长和安装校准保存选项；YAML 写入由保存操作触发。
- 正式窗口检查 Light 与恢复 Slime 主题、800 × 560 窗口；主题恢复后画面与切换前一致。
- 独立预览检查三种主题、12 / 16 / 20px 字号、560 × 650 窗口与滚动、长中英文、控件交互和禁用状态。主题恢复后画面一致，预览状态维持在窗口内存中。
- Windows 11 的输入、主题和布局按实机窗口检查。

## 组件范围

组件库当前覆盖上表中的设置控件。弹窗、设备卡片、保存栏、输入框和下拉菜单由各页面组合。提取更多组件时先明确受控状态和交互回调，再统一样式。追踪器信息、分配与骨架等组件按 SlimeVR 的业务接口组织。

组件化统一样式与维护入口。CPU 和内存收益通过等价场景的独立性能测量评估。
