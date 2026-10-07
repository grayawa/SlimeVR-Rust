pub mod client;
pub mod host;
pub mod i18n;
pub mod navigation;
pub mod protocol;

#[path = "../../shared/log_level.rs"]
pub mod log_level;

pub mod avatar;
pub mod desktop;
pub mod exit_warning;
pub mod firmware;
pub mod json_form;
pub mod locales;
pub mod logging;
pub mod presence;
pub mod proportions;
pub mod rpc_generated;
pub mod settings;
pub mod sounds;
pub mod tray;
pub mod visualization;

pub mod assignment;
pub mod battery;

pub mod checklist;
pub mod dashboard;
pub mod overlay;
pub mod tracker_list;

pub mod settings_layout;

pub mod mounting;
pub mod onboarding;

#[cfg(feature = "desktop")]
pub mod ui;
