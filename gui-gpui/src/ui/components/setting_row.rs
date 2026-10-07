use super::card;
use crate::ui::theme::{self, Surface};
use gpui_kit::component::{ActiveTheme, Disableable, StyledExt, switch::Switch};
use gpui_kit::{prelude::FluentBuilder as _, *};

/// Label and supplied native switch; the owner wires value and callback.
pub struct SwitchRow {
    label: SharedString,
    switch: Switch,
    disabled: bool,
}
impl SwitchRow {
    pub fn new(label: impl Into<SharedString>, switch: Switch) -> Self {
        let label = label.into();
        Self {
            switch: switch.accessibility_label(label.clone()),
            label,
            disabled: false,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.switch = self.switch.disabled(disabled);
        self
    }
    pub fn build(self, cx: &App) -> Div {
        card(Surface::Control, cx)
            .h_flex()
            .w_full()
            .gap(theme::CONTROL_GAP)
            .items_center()
            .p(theme::CONTROL_PADDING)
            .min_h(theme::SWITCH_MIN_HEIGHT)
            .when(self.disabled, |d| d.text_color(cx.theme().muted_foreground))
            .child(self.switch)
            .child(div().flex_1().min_w_0().child(self.label))
    }
}
