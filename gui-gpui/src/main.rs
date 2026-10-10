#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod ui_assets;
mod view;
use clap::Parser;
use gpui_kit::*;
use slimevr_gpui::{
    client::Client,
    desktop::{Paths, Preferences, SingleInstance},
    host::Backend,
    i18n::Localizer,
    log_level::LogLevel,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "SlimeVR-Rust native frontend — Independent Development Preview"
)]
struct Options {
    #[arg(long, default_value = "ws://127.0.0.1:21110")]
    url: String,
    #[arg(long)]
    locale: Option<String>,
    #[arg(long)]
    attach: bool,
    /// Optionally start this backend executable. Otherwise attach to an existing service.
    #[arg(long)]
    backend: Option<PathBuf>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long, value_enum)]
    log_level: Option<LogLevel>,
}

fn main() {
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("Native application panic");
            Err(message.to_owned())
        });
    if let Err(error) = result {
        slimevr_gpui::logging::write(LogLevel::Error, "startup", &error);
        eprintln!("{error}");
        #[cfg(windows)]
        {
            let title: Vec<u16> = "SlimeVR-Rust".encode_utf16().chain(Some(0)).collect();
            let message: Vec<u16> = error.encode_utf16().chain(Some(0)).collect();
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    0x10,
                );
            }
        }
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let options = Options::parse();
    let level = LogLevel::resolve(options.log_level, false)?;
    let paths = Paths::new()?;
    let Some(instance) = SingleInstance::acquire(&paths)? else {
        return Ok(());
    };
    let mut preferences = Preferences::load(&paths)?;
    let locale = options.locale.clone().unwrap_or_else(|| {
        preferences.value["lang"]
            .as_str()
            .unwrap_or("zh-Hans")
            .to_owned()
    });
    preferences.value["lang"] = serde_json::json!(locale);
    slimevr_gpui::logging::init(paths.logs.clone(), level);
    slimevr_gpui::logging::write(
        LogLevel::Info,
        "startup",
        &format!(
            "SlimeVR-Rust GPUI {} — Independent Development Preview",
            env!("CARGO_PKG_VERSION")
        ),
    );
    std::panic::set_hook(Box::new(|info| {
        slimevr_gpui::logging::write(LogLevel::Error, "panic", &info.to_string());
        eprintln!("{info}");
    }));
    let mut l10n = Localizer::new(&locale)?;
    if let Ok(source) = std::fs::read_to_string(paths.root.join("override.ftl"))
        && let Err(error) = l10n.add_override(&source)
    {
        slimevr_gpui::logging::write(LogLevel::Warn, "i18n", &error);
    }
    let bundled = paths.resources.join(if cfg!(windows) {
        "slimevr-server.exe"
    } else {
        "slimevr-server"
    });
    let already_running = options
        .url
        .strip_prefix("ws://")
        .and_then(|a| a.trim_end_matches('/').parse().ok())
        .is_some_and(|a| {
            std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(300)).is_ok()
        });
    let backend_path = options.backend.clone().or_else(|| {
        if !options.attach && !already_running && bundled.is_file() {
            Some(bundled)
        } else {
            None
        }
    });
    let backend = backend_path
        .as_ref()
        .map(|path| {
            Backend::start(
                path,
                options.config.as_deref().or(Some(paths.config.as_path())),
                &options.url,
                level,
            )
        })
        .transpose()?;
    let client = Client::connect(options.url.clone(), level)?;
    gpui_kit::application()
        .with_assets(ui_assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let fonts = vec![
                include_bytes!("../assets/fonts/Poppins-Regular.ttf").as_slice(),
                include_bytes!("../assets/fonts/Poppins-Bold.ttf").as_slice(),
                include_bytes!("../assets/fonts/Ubuntu-B.ttf").as_slice(),
                include_bytes!("../assets/fonts/OpenDyslexic-Bold.ttf").as_slice(),
                include_bytes!("../assets/fonts/noto-sans-v42-latin-regular.ttf").as_slice(),
                include_bytes!("../assets/fonts/Lexend[HEXP,wght].ttf").as_slice(),
                include_bytes!("../assets/fonts/Ubuntu-R.ttf").as_slice(),
                include_bytes!("../assets/fonts/OpenDyslexic-Regular.ttf").as_slice(),
            ];
            if let Err(error) = cx
                .text_system()
                .add_fonts(fonts.into_iter().map(std::borrow::Cow::Borrowed).collect())
            {
                slimevr_gpui::logging::write(LogLevel::Warn, "fonts", &error.to_string());
            }
            gpui_kit::component::set_locale(if locale == "zh-Hans" { "zh-CN" } else { "en" });
            slimevr_gpui::theme::apply_theme(
                preferences.value["theme"].as_str().unwrap_or("slime"),
                cx,
            );
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
                window_min_size: Some(size(px(800.), px(560.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("SlimeVR-Rust — GPUI — Independent Development Preview".into()),
                    ..gpui_kit::component::TitleBar::title_bar_options()
                }),
                ..gpui_kit::component::TitleBar::window_options()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, |_, cx| {
                cx.new(|cx| {
                    view::SlimeView::new(client, backend, l10n, paths, preferences, instance, cx)
                })
            }) {
                eprintln!("[error] Unable to open native window: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}
