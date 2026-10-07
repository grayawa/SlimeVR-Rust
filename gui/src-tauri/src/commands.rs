use crate::{paths::AppPaths, server::LaunchOptions};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn os_stats() -> Value {
    json!({ "type": match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => "unknown",
    } })
}

#[tauri::command]
pub fn is_steam(options: State<'_, LaunchOptions>) -> bool {
    options.steam
}

#[tauri::command]
pub fn install_dir(paths: State<'_, AppPaths>) -> String {
    paths.resources.to_string_lossy().into_owned()
}

#[tauri::command]
pub fn i18n_override(paths: State<'_, AppPaths>) -> Result<Value, String> {
    match std::fs::read_to_string(paths.server.join("override.ftl")) {
        Ok(text) => Ok(Value::String(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Bool(false)),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn open_folder(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    folder: String,
) -> Result<(), String> {
    let path = match folder.as_str() {
        "config" => &paths.server,
        "logs" => &paths.logs,
        _ => return Err("Unknown application folder".into()),
    };
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_managed_path(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    path: String,
) -> Result<(), String> {
    if !paths.allows_open(std::path::Path::new(&path)) {
        return Err("Path is outside SlimeVR configuration and log folders".into());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_log(
    logger: State<'_, crate::logging::Logger>,
    level: String,
    args: Vec<Value>,
) -> Result<(), String> {
    logger.append(crate::log_level::LogLevel::parse(&level)?, "gui", &args)
}

#[tauri::command]
pub fn log_level(logger: State<'_, crate::logging::Logger>) -> &'static str {
    logger.level().as_str()
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum GithubRequest {
    #[serde(rename = "fw-releases")]
    FirmwareReleases,
    #[serde(rename = "asset")]
    Asset { url: String },
}

fn allowed_asset_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url
            .path()
            .starts_with("/SlimeVR/SlimeVR-Tracker-ESP/releases/download/")
}

#[tauri::command]
pub async fn github_get(options: GithubRequest) -> Result<Value, String> {
    let url = match options {
        GithubRequest::FirmwareReleases => {
            "https://api.github.com/repos/SlimeVR/SlimeVR-Tracker-ESP/releases".to_owned()
        }
        GithubRequest::Asset { url } => {
            let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
            if !allowed_asset_url(&parsed) {
                return Err("Not an official SlimeVR firmware asset URL".into());
            }
            url
        }
    };
    let client = reqwest::Client::builder()
        .user_agent("SlimeVR-Tauri")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_asset_scope_rejects_unrelated_hosts_and_paths() {
        for url in [
            "http://github.com/SlimeVR/SlimeVR-Tracker-ESP/releases/download/v1/deploy.json",
            "https://github.com.evil.example/SlimeVR/SlimeVR-Tracker-ESP/releases/download/v1/deploy.json",
            "https://github.com/other/repo/releases/download/v1/deploy.json",
            "https://github.com/SlimeVR/SlimeVR-Tracker-ESP/releases/download/../other",
        ] {
            assert!(!allowed_asset_url(&reqwest::Url::parse(url).unwrap()));
        }
        assert!(allowed_asset_url(
            &reqwest::Url::parse(
                "https://github.com/SlimeVR/SlimeVR-Tracker-ESP/releases/download/v1/deploy.json"
            )
            .unwrap()
        ));
    }
}
