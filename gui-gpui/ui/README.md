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

组件与应用共享 `gui-gpui/Cargo.lock`。从仓库根目录执行：

```sh
cargo check --manifest-path gui-gpui/ui/Cargo.toml --locked
cargo clippy --manifest-path gui-gpui/ui/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path gui-gpui/Cargo.toml -p slimevr-gpui -p slimevr-ui --check
cargo run --manifest-path gui-gpui/Cargo.toml --bin slimevr-gpui-components --locked
```

编译环境需要 GPUI Kit 对应平台的开发依赖，见
[安装说明](https://gpui-kit.com/docs/installation)。组件预览由应用 crate 提供，
展示主题、字号、长文案、窄窗口和控件交互。接口与页面职责见
[组件库说明](../../docs/rust-gpui-components.zh-CN.md)。

本 crate 使用 `GPL-3.0-or-later`，条款见 [LICENSE](LICENSE)。项目许可范围见
[LICENSING.md](../../LICENSING.md)。
