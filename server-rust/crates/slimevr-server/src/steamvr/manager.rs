//! Original SteamVR HTTP driver controls and bundled-driver registration via vrpathreg.
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationOutcome {
    Skipped,
    Existing,
    ExistingManualDriver,
    Installed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverNotice {
    ExistingManualDriver,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DriverStatus {
    pub known: bool,
    pub installed: bool,
    pub enabled: bool,
    pub blocked: bool,
    pub standable_installed: bool,
    pub registration_error: Option<String>,
    pub registration_notice: Option<DriverNotice>,
}
#[derive(Clone)]
pub struct Manager {
    client: reqwest::Client,
    pub url: String,
    pub source: Option<PathBuf>,
    pub runtime: Option<PathBuf>,
    pub install: bool,
    pub restart: bool,
}
#[derive(Deserialize)]
struct List {
    jsonid: String,
    drivers: Vec<Driver>,
}
#[derive(Deserialize)]
struct Driver {
    enabled: bool,
    blocked_by_safe_mode: bool,
    manifest: Manifest,
}
#[derive(Deserialize)]
struct Manifest {
    name: String,
}
impl Manager {
    pub fn new(
        source: Option<PathBuf>,
        runtime: Option<PathBuf>,
        url: Option<String>,
        install: bool,
        restart: bool,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?;
        let url = url.unwrap_or_else(|| "http://127.0.0.1:27062".into());
        let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
        if parsed.scheme() != "http"
            || !matches!(
                parsed.host_str(),
                Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
            )
        {
            return Err("SteamVR management endpoint must be a local HTTP address".into());
        }
        Ok(Self {
            client,
            url,
            source: source.or_else(find_source),
            runtime: runtime.or_else(find_runtime),
            install,
            restart,
        })
    }
    pub async fn status(&self) -> Result<DriverStatus, String> {
        let response = self
            .client
            .get(format!("{}/drivers/list.json", self.url))
            .header("Referer", format!("{}/dashboard/index.html", self.url))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("SteamVR driver list exceeds 1 MiB".into());
        }
        let list: List = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if list.jsonid != "vr_driver_list" {
            return Err("unexpected SteamVR driver list identifier".into());
        }
        let standable_installed = list.drivers.iter().any(|d| d.manifest.name == "standable");
        let d = list
            .drivers
            .into_iter()
            .find(|d| d.manifest.name == "slimevr");
        Ok(match d {
            Some(d) => DriverStatus {
                known: true,
                installed: true,
                enabled: d.enabled,
                blocked: d.blocked_by_safe_mode,
                standable_installed,
                registration_error: None,
                registration_notice: None,
            },
            None => DriverStatus {
                known: true,
                standable_installed,
                ..Default::default()
            },
        })
    }
    pub async fn enable(&self) -> Result<DriverStatus, String> {
        for (route, body) in [
            ("unblock", serde_json::json!({"driver":"slimevr"})),
            (
                "setenable",
                serde_json::json!({"driver":"slimevr","enable":true}),
            ),
        ] {
            self.client
                .post(format!("{}/drivers/{route}", self.url))
                .header("Referer", format!("{}/dashboard/index.html", self.url))
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
        }
        let status = self.status().await?;
        if !status.installed || !status.enabled || status.blocked {
            return Err("SteamVR did not confirm that the SlimeVR driver was enabled".into());
        }
        if self.restart {
            tokio::time::sleep(Duration::from_millis(500)).await;
            open_restart().await?;
        }
        Ok(status)
    }
    pub async fn register(&self) -> Result<RegistrationOutcome, String> {
        if !self.install || cfg!(target_os = "macos") {
            return Ok(RegistrationOutcome::Skipped);
        }
        let Some(runtime) = &self.runtime else {
            return Ok(RegistrationOutcome::Skipped);
        };
        let executable = runtime.join(if cfg!(windows) {
            "bin/win64/vrpathreg.exe"
        } else {
            "bin/vrpathreg.sh"
        });
        if !executable.is_file() {
            return Err("SteamVR vrpathreg executable is missing".into());
        }
        let find = run(
            &executable,
            &[
                std::ffi::OsStr::new("finddriver"),
                std::ffi::OsStr::new("slimevr"),
            ],
        )
        .await?;
        if find == 0 {
            return Ok(RegistrationOutcome::Existing);
        }
        if find != 1 {
            return Err(format!(
                "vrpathreg finddriver exited with {find}; registration was not changed"
            ));
        }
        if runtime.join("drivers/slimevr").exists() {
            return Ok(RegistrationOutcome::ExistingManualDriver);
        }
        let source = self
            .source
            .as_ref()
            .ok_or("bundled SlimeVR driver is missing; supply --steamvr-driver")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let manifest =
            std::fs::read(source.join("driver.vrdrivermanifest")).map_err(|e| e.to_string())?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&manifest).map_err(|e| e.to_string())?;
        if manifest["name"] != "slimevr" {
            return Err("driver manifest is not a SlimeVR driver".into());
        }
        let exit = run(
            &executable,
            &[std::ffi::OsStr::new("adddriver"), source.as_os_str()],
        )
        .await?;
        if exit != 0 {
            return Err(format!("vrpathreg adddriver exited with {exit}"));
        }
        Ok(RegistrationOutcome::Installed)
    }
}
async fn run(executable: &Path, args: &[&std::ffi::OsStr]) -> Result<i32, String> {
    let mut command = tokio::process::Command::new(executable);
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(10), command.status())
        .await
        .map_err(|_| "SteamVR command timed out".to_string())?
        .map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(-1))
}
async fn open_restart() -> Result<(), String> {
    let (program, args) = if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", "vrmonitor://restartsystem"])
    } else if cfg!(target_os = "macos") {
        ("open", vec!["vrmonitor://restartsystem"])
    } else {
        ("xdg-open", vec!["vrmonitor://restartsystem"])
    };
    let args: Vec<_> = args.iter().map(std::ffi::OsStr::new).collect();
    if run(Path::new(program), &args).await? != 0 {
        return Err("Unable to request SteamVR restart".into());
    }
    Ok(())
}
pub fn find_runtime() -> Option<PathBuf> {
    let root = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
    }?;
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("openvr/openvrpaths.vrpath")).ok()?)
            .ok()?;
    config["runtime"]
        .as_array()?
        .iter()
        .filter_map(|p| p.as_str())
        .map(PathBuf::from)
        .find(|p| p.is_dir())
}
pub fn find_source() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "slimevr-openvr-driver-win64"
    } else if cfg!(target_arch = "aarch64") {
        "slimevr-openvr-driver-aarch64-linux"
    } else {
        "slimevr-openvr-driver-x64-linux"
    };
    let mut roots = vec![
        PathBuf::from("."),
        PathBuf::from("gui/src-tauri/resources/drivers"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.to_owned());
            roots.push(parent.join("drivers"));
        }
    }
    roots
        .into_iter()
        .map(|p| p.join(name))
        .find(|p| p.join("driver.vrdrivermanifest").is_file())
}
