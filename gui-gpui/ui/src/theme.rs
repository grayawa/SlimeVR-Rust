//! Live theme roles and dimensions shared by settings components.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Hsla, Pixels, px};

#[derive(Clone, Copy, Default)]
pub enum Surface {
    Panel,
    #[default]
    Control,
}
impl Surface {
    /// Resolve the surface color from the current theme on each render/build.
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            Self::Panel => cx.theme().muted,
            Self::Control => cx.theme().popover,
        }
    }
}

pub const CARD_RADIUS: Pixels = px(8.);
pub const CONTROL_PADDING: Pixels = px(12.);
pub const CONTROL_GAP: Pixels = px(12.);
pub const LABEL_GAP: Pixels = px(8.);
pub const COMPACT_PADDING: Pixels = px(8.);
pub const CHOICE_MIN_HEIGHT: Pixels = px(88.);
pub const SWITCH_MIN_HEIGHT: Pixels = px(50.);
pub const PANE_PADDING: Pixels = px(16.);
pub const PANE_VERTICAL_PADDING: Pixels = px(32.);
pub const PANE_GAP: Pixels = px(16.);
