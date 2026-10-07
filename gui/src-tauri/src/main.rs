#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
#[path = "../../../shared/log_level.rs"]
mod log_level;
mod logging;
mod paths;
mod presence;
mod server;

use clap::Parser;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    let options = server::LaunchOptions::parse();
    let log_level =
        log_level::LogLevel::resolve(options.log_level, false).unwrap_or_else(|error| {
            use clap::error::ErrorKind;
            clap::Error::raw(
                ErrorKind::InvalidValue,
                format!("SLIMEVR_LOG_LEVEL: {error}"),
            )
            .exit()
        });
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(options)
        .manage(server::ServerProcess::default())
        .invoke_handler(tauri::generate_handler![
            presence::set_presence,
            commands::os_stats,
            commands::install_dir,
            commands::i18n_override,
            commands::open_folder,
            commands::open_managed_path,
            commands::write_log,
            commands::log_level,
            commands::github_get,
            server::server_status_history,
        ])
        .setup(move |app| {
            let paths = paths::AppPaths::new(app.handle())?;
            let logger = logging::Logger::new(paths.logs.clone(), log_level);
            let _ = logger.append(
                log_level::LogLevel::Info,
                "desktop",
                &[serde_json::json!({"type":"logging_started", "log_level":log_level})],
            );
            app.manage(logger);
            app.manage(paths);
            app.manage(presence::Presence::start());
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &hide, &quit])?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("SlimeVR icon is configured")
                        .clone(),
                )
                .tooltip("SlimeVR")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            server::start(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Failed to initialize the SlimeVR Tauri host");
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            app.state::<server::ServerProcess>().stop();
        }
    });
}
