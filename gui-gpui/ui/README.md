# slimevr-ui

`slimevr-ui` 提供使用 GPUI Kit 0.7.1 的受控设置组件。调用方传入文案、状态、
显示值与交互控件，并通过回调更新自己的状态。

| 导出                         | 用途                           |
| ---------------------------- | ------------------------------ |
| `components::card`           | 使用当前主题的卡片表面         |
| `components::SettingsPane`   | 标题、图标与内容区域           |
| `components::ChoiceCard`     | 单选卡片、说明、选中与禁用状态 |
| `components::NumberSelector` | 数值显示与两端加减按钮         |
| `components::SwitchRow`      | 标签与原生开关排列             |
| `theme::Surface`、尺寸常量   | 当前主题颜色角色与共用布局尺寸 |

## 组件与调用方职责

| 组件                | 组件负责                                             | 页面负责                                                  |
| ------------------- | ---------------------------------------------------- | --------------------------------------------------------- |
| SettingsPane / card | 背景、圆角、区域布局、标题及图标容器                 | 文案翻译、图标素材和页面内容                              |
| ChoiceCard          | 卡片布局、选中边框、禁用样式、实际宽度换行和指定字号 | 选中值、选项意义、点击后的状态修改                        |
| NumberSelector      | 标签、数值条及两端按钮排列                           | 单位格式、整数/浮点转换、步长、上下限、禁用按钮和修改回调 |
| SwitchRow           | 标签、开关排列、可访问名称、整行禁用                 | 开关当前值、确认流程和修改回调                            |

`Surface::Panel` 对应页面面板背景，`Surface::Control` 对应控件卡片背景。颜色在组件构建时从当前主题读取，圆角、内边距、间距和最低高度集中在 `src/theme.rs`。`build(cx)` 返回 GPUI 元素，调用方继续绑定回调和组合布局。

## 接入

其他 GPUI 项目可以通过本地 path 依赖接入：

```toml
[dependencies]
gpui-kit = "=0.7.1"
slimevr-ui = { path = "/path/to/SlimeVR-Rust/gui-gpui/ui" }
```

在页面中组合组件并绑定事件：

```rust,ignore
use gpui_kit::*;
use slimevr_ui::components::ChoiceCard;

let choice = ChoiceCard::new("smoothing", "平滑型")
    .description(Some("让运动更加平滑，增加少量延迟。".into()))
    .checked(self.smoothing)
    .disabled(disabled)
    .text_size(px(preferred_text_size))
    .build(cx)
    .on_click(cx.listener(|this, _, _, cx| {
        this.smoothing = true;
        cx.notify();
    }));
```

消费应用负责 GPUI Kit 初始化、窗口、字体、翻译、主题和业务状态。
组件在 `build(cx)` 时读取当前主题；主题修改后，调用方触发重新渲染。
数值的单位、步长、范围与保存规则由调用方提供。

本仓库的 `gui-gpui/src/theme.rs` 提供 SlimeVR 主题预设。Windows 应用使用
`gui-gpui/Cargo.toml` 中的 renderer patch；独立消费应用按自己的 workspace
根 manifest 配置 GPUI 平台依赖与所需补丁。

## 开发

组件与应用共享 `gui-gpui/Cargo.lock`、GPUI Kit 版本和根 manifest 的 Windows renderer patch。`gui-gpui/Cargo.toml` 的 workspace 成员为应用和 `ui`，默认成员为应用；应用通过 `desktop` feature 引入组件。通信与状态检查使用应用的 `--no-default-features`。OpenVR runtime 和 vendor 使用各自 manifest。

从仓库根目录执行：

```sh
cargo check --manifest-path gui-gpui/ui/Cargo.toml --locked
cargo clippy --manifest-path gui-gpui/ui/Cargo.toml --all-targets --locked -- -D warnings
cargo clippy --manifest-path gui-gpui/Cargo.toml --workspace --all-targets --features vr-dashboard --locked -- -D warnings
cargo fmt --manifest-path gui-gpui/Cargo.toml -p slimevr-gpui -p slimevr-ui --check
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components --locked
```

编译环境需要 GPUI Kit 对应平台的开发依赖，见
[安装说明](https://gpui-kit.com/docs/installation)。组件预览由应用 crate 提供，
展示主题、字号、长文案、窄窗口和控件交互。SlimeVR 应用接入见
[GPUI 指南](../../docs/rust-gpui-guide.zh-CN.md#组件和生成数据)。

## 组件预览

```sh
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components --locked -- --locale en --theme light --text-size 16
```

Windows 包中的 `SlimeVR-Components.exe` 使用同一组件库。预览提供 Slime / Light / Green、12 / 16 / 20px 字号、数值条、开关、选中 / 禁用状态与长中英文说明；窄窗口中单选卡纵向排列，内容可滚动。示例值和交互保存在窗口内存中，正式页面的参数范围由设置描述提供。

组件检查覆盖独立编译、workspace Clippy、桌面 / Overlay / 预览接入与共享锁文件。窗口验收检查主题切换再恢复、字号、560 × 650 窄窗口、滚动、长文案和禁用状态；正式页面交互按 [实机清单](../../docs/rust-unified-hardware-test.zh-CN.md#gpui-页面与桌面交互) 检查。

## 扩展

当前导出覆盖上表中的设置控件，其他弹窗、设备卡片、保存栏、输入框和下拉菜单由调用页面组合。提取更多组件时先确定受控状态和回调，再统一布局与主题。组件性能按等价场景单独测量。

本 crate 使用 `GPL-3.0-or-later`，条款见 [LICENSE](LICENSE)。项目许可范围见
[LICENSING.md](../../LICENSING.md)。
