//! SlimeVR theme presets selected by the application.
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, rgb};

pub fn apply_theme(name: &str, cx: &mut App) {
    Theme::change(
        if name == "light" {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    let primary = match name {
        "slime-green" => 0x69ba82,
        "slime-yellow" => 0xd7b65c,
        "slime-orange" => 0xd49860,
        "slime-red" => 0xc96f7c,
        "trans" => 0x83cbdc,
        "asexual" => 0x9773b9,
        "snep" => 0x8babb6,
        _ => 0x65459a,
    };
    Theme::update(cx, |theme| {
        theme.colors.primary = rgb(primary).into();
        theme.colors.primary_foreground = rgb(0xffffff).into();
        theme.colors.button_primary = rgb(primary).into();
        theme.colors.button_primary_foreground = rgb(0xffffff).into();
        theme.colors.button_primary_hover = rgb(primary).into();
        theme.colors.button_primary_active = rgb(primary).into();
        if name.starts_with("slime") {
            theme.colors.background = rgb(0x00101c).into();
            theme.colors.foreground = rgb(0xe9eef2).into();
            theme.colors.muted = rgb(0x081e30).into();
            theme.colors.link = rgb(0xbb8ae5).into();
            theme.colors.muted_foreground = rgb(0x78a4c6).into();
            theme.colors.secondary = rgb(0x2e2145).into();
            theme.colors.border = rgb(0x1a3d59).into();
            theme.colors.input = rgb(0x00101c).into();
            theme.colors.popover = rgb(0x112d43).into();
            theme.colors.switch = rgb(0x1a3d59).into();
            theme.colors.switch_thumb = rgb(0xc0a1d8).into();
        }
    });
}
