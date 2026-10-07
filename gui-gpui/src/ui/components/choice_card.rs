use crate::ui::theme::{self, Surface};
use gpui_kit::component::{
    ActiveTheme, Disableable, StyledExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::FluentBuilder as _, *};

/// Controlled single-choice presentation; selection and its effect belong to the owner.
pub struct ChoiceCard {
    id: ElementId,
    title: SharedString,
    description: Option<SharedString>,
    checked: bool,
    disabled: bool,
    text_size: Pixels,
}
impl ChoiceCard {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            checked: false,
            disabled: false,
            text_size: px(12.),
        }
    }
    pub fn description(mut self, description: Option<SharedString>) -> Self {
        self.description = description;
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn text_size(mut self, text_size: Pixels) -> Self {
        self.text_size = text_size;
        self
    }
    /// Returns a native button so callbacks, keyboard activation and focus stay with GPUI Kit.
    pub fn build(self, cx: &App) -> Button {
        let background = Surface::Control.color(cx);
        Button::new(self.id)
            .accessibility_label(self.title.clone())
            .ghost()
            .w_full()
            .h_auto()
            .min_h(theme::CHOICE_MIN_HEIGHT)
            .p(theme::CONTROL_PADDING)
            .rounded(theme::CARD_RADIUS)
            .bg(background)
            .border_2()
            .border_color(if self.checked {
                cx.theme().primary
            } else {
                background
            })
            .disabled(self.disabled)
            .child(
                div()
                    .h_flex()
                    .text_size(self.text_size)
                    .w_full()
                    .gap(theme::CONTROL_GAP)
                    .items_start()
                    .child(if self.checked { "◉" } else { "○" })
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .gap(theme::LABEL_GAP)
                            .child(div().font_bold().child(self.title))
                            .when_some(self.description, |d, description| {
                                d.child(div().w_full().whitespace_normal().child(description))
                            }),
                    ),
            )
    }
}
