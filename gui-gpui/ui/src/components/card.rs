use crate::theme::{self, Surface};
use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::{prelude::FluentBuilder as _, *};

/// A themed surface on which callers compose their content layout.
pub fn card(surface: Surface, cx: &App) -> Div {
    div().rounded(theme::CARD_RADIUS).bg(surface.color(cx))
}

/// A settings pane with a heading badge and content column.
pub struct SettingsPane {
    title: SharedString,
    icon: AnyElement,
    content: AnyElement,
}
impl SettingsPane {
    pub fn new(icon: impl IntoElement, content: impl IntoElement) -> Self {
        Self {
            title: "".into(),
            icon: icon.into_any_element(),
            content: content.into_any_element(),
        }
    }
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }
    pub fn build(self, cx: &App) -> Div {
        card(Surface::Panel, cx)
            .h_flex()
            .items_start()
            .gap(theme::PANE_GAP)
            .w_full()
            .p(theme::PANE_PADDING)
            .py(theme::PANE_VERTICAL_PADDING)
            .child(
                div()
                    .size(px(40.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.icon),
            )
            .child(
                div()
                    .v_flex()
                    .gap(theme::CONTROL_GAP)
                    .flex_1()
                    .min_w_0()
                    .when(!self.title.is_empty(), |d| {
                        d.child(div().text_2xl().font_bold().child(self.title))
                    })
                    .child(self.content),
            )
    }
}
