use super::card;
use crate::theme::{self, Surface};
use gpui_kit::component::{Sizable, StyledExt, button::Button};
use gpui_kit::*;

/// Numeric control layout with caller-supplied display text and increment/decrement buttons.
pub struct NumberSelector {
    label: SharedString,
    display: SharedString,
    decrease: Button,
    increase: Button,
}
impl NumberSelector {
    pub fn new(
        label: impl Into<SharedString>,
        display: impl Into<SharedString>,
        decrease: Button,
        increase: Button,
    ) -> Self {
        Self {
            label: label.into(),
            display: display.into(),
            decrease,
            increase,
        }
    }
    pub fn decrement(id: impl Into<ElementId>) -> Button {
        Button::new(id).small().label("−")
    }
    pub fn increment(id: impl Into<ElementId>) -> Button {
        Button::new(id).small().label("+")
    }
    pub fn build(self, cx: &App) -> Div {
        div()
            .v_flex()
            .gap(theme::LABEL_GAP)
            .w_full()
            .child(div().font_bold().child(self.label))
            .child(
                card(Surface::Control, cx)
                    .h_flex()
                    .gap(theme::CONTROL_GAP)
                    .p(theme::COMPACT_PADDING)
                    .items_center()
                    .child(self.decrease)
                    .child(div().flex_1().text_center().child(self.display))
                    .child(self.increase),
            )
    }
}
