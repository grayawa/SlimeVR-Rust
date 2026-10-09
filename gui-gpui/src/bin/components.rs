//! Interactive component catalogue. No backend, preferences or user files are opened.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use clap::Parser;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    switch::Switch,
};
use gpui_kit::{prelude::FluentBuilder as _, *};
use slimevr_gpui::{
    i18n::Localizer,
    ui::{
        components::{ChoiceCard, NumberSelector, SettingsPane, SwitchRow},
        theme,
    },
};

#[derive(Parser)]
#[command(about = "SlimeVR component preview; all interactions stay in memory")]
struct Options {
    #[arg(long, default_value = "slime")]
    theme: String,
    #[arg(long, default_value = "zh-Hans")]
    locale: String,
    #[arg(long, default_value_t = 12, value_parser = clap::value_parser!(u16).range(8..=25))]
    text_size: u16,
}
struct Preview {
    l10n: Localizer,
    theme: String,
    text_size: f32,
    choice: u8,
    amount: u8,
    seconds: u16,
    enabled: bool,
}
impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut toolbar = div()
            .h_flex()
            .flex_wrap()
            .gap_3()
            .items_center()
            .child("SlimeVR · 组件预览 / Components");
        for (name, label) in [
            ("slime", "Slime"),
            ("light", "Light"),
            ("slime-green", "Green"),
        ] {
            toolbar = toolbar.child(
                Button::new(format!("preview-theme-{name}"))
                    .label(label)
                    .when(self.theme == name, |b| b.primary())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.theme = name.into();
                        theme::apply_theme(name, cx);
                        cx.notify();
                    })),
            );
        }
        for font_size in [12, 16, 20] {
            toolbar = toolbar.child(
                Button::new(format!("preview-font-{font_size}"))
                    .label(format!("{font_size}px"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.text_size = font_size as f32;
                        cx.notify();
                    })),
            );
        }
        let wide = f32::from(window.viewport_size().width) >= 1000.;
        let mut choices = div()
            .flex()
            .w_full()
            .gap_3()
            .when(wide, |d| d.flex_row())
            .when(!wide, |d| d.flex_col());
        for (index, key) in ["none", "smoothing", "prediction"].into_iter().enumerate() {
            let button = ChoiceCard::new(
                format!("preview-choice-{key}"),
                self.l10n.text(&format!(
                    "settings-general-tracker_mechanics-filtering-type-{key}"
                )),
            )
            .description(Some(
                self.l10n
                    .text(&format!(
                        "settings-general-tracker_mechanics-filtering-type-{key}-description"
                    ))
                    .into(),
            ))
            .checked(self.choice == index as u8)
            .text_size(px(self.text_size))
            .build(cx)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.choice = index as u8;
                cx.notify();
            }));
            choices = choices.child(div().min_w_0().when(wide, |d| d.flex_1()).child(button));
        }
        let decrease = NumberSelector::decrement("preview-amount-decrease")
            .disabled(self.amount == 0)
            .on_click(cx.listener(|this, _, _, cx| {
                this.amount = this.amount.saturating_sub(5);
                cx.notify();
            }));
        let increase = NumberSelector::increment("preview-amount-increase")
            .disabled(self.amount == 100)
            .on_click(cx.listener(|this, _, _, cx| {
                this.amount = this.amount.saturating_add(5).min(100);
                cx.notify();
            }));
        let amount = NumberSelector::new(
            self.l10n
                .text("settings-general-tracker_mechanics-filtering-amount"),
            format!("{}%", self.amount),
            decrease,
            increase,
        )
        .build(cx);
        let decrease = NumberSelector::decrement("preview-seconds-decrease")
            .disabled(self.seconds == 0)
            .on_click(cx.listener(|this, _, _, cx| {
                this.seconds = this.seconds.saturating_sub(25);
                cx.notify();
            }));
        let increase = NumberSelector::increment("preview-seconds-increase")
            .disabled(self.seconds >= 1000)
            .on_click(cx.listener(|this, _, _, cx| {
                this.seconds = this.seconds.saturating_add(25).min(1000);
                cx.notify();
            }));
        let seconds = NumberSelector::new(
            self.l10n
                .text("settings-general-tracker_mechanics-yaw-reset-smooth-time"),
            format!("{:.2}s", self.seconds as f32 / 100.),
            decrease,
            increase,
        )
        .build(cx);
        let label = self
            .l10n
            .text("settings-general-tracker_mechanics-save_mounting_reset-enabled-label");
        let switch = Switch::new("preview-switch")
            .small()
            .checked(self.enabled)
            .accessibility_label(label.clone())
            .on_click(cx.listener(|this, enabled, _, cx| {
                this.enabled = *enabled;
                cx.notify();
            }));
        let switch = SwitchRow::new(label, switch).build(cx);
        let disabled = SwitchRow::new(
            "禁用 / Disabled",
            Switch::new("preview-disabled-switch")
                .small()
                .disabled(true)
                .accessibility_label("禁用 / Disabled"),
        )
        .disabled(true)
        .build(cx);
        let disabled_choice = ChoiceCard::new("preview-disabled-choice", "禁用的单选卡 / Disabled choice")
            .description(Some("这是一段用于检查长中文和 English wrapping 的说明。缩窄窗口、增大字号或切换主题后，卡片应保持可读，操作不会修改任何软件配置。".into()))
            .text_size(px(self.text_size)).disabled(true).build(cx);
        let content = div()
            .v_flex()
            .gap_5()
            .w_full()
            .child(choices)
            .child(amount)
            .child(seconds)
            .child(switch)
            .child(disabled)
            .child(disabled_choice);
        div()
            .v_flex()
            .size_full()
            .p_4()
            .gap_4()
            .font_family("Poppins")
            .text_size(px(self.text_size))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(toolbar)
            .child(
                div()
                    .id("preview-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        SettingsPane::new(div().font_bold().child("S"), content)
                            .title("设置组件 / Settings components")
                            .build(cx),
                    ),
            )
    }
}
fn main() {
    let options = Options::parse();
    let l10n = Localizer::new(&options.locale).expect("Preview locale");
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.text_system()
                .add_fonts(vec![
                    std::borrow::Cow::Borrowed(
                        include_bytes!("../../assets/fonts/Poppins-Regular.ttf").as_slice(),
                    ),
                    std::borrow::Cow::Borrowed(
                        include_bytes!("../../assets/fonts/Poppins-Bold.ttf").as_slice(),
                    ),
                ])
                .expect("Preview fonts");
            theme::apply_theme(&options.theme, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = WindowBounds::centered(size(px(1100.), px(800.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(bounds),
                    window_min_size: Some(size(px(480.), px(400.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(
                            "SlimeVR-Rust — Components — Independent Development Preview".into(),
                        ),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|_| Preview {
                        l10n,
                        theme: options.theme,
                        text_size: options.text_size as f32,
                        choice: 1,
                        amount: 20,
                        seconds: 0,
                        enabled: false,
                    })
                },
            )
            .expect("Preview window");
            cx.activate(true);
        });
}
