use crate::{
    log_level::LogLevel,
    logging::{status_level, Logger},
    paths::AppPaths,
};
use clap::{Parser, ValueEnum};
use serde::Serialize;
use serde_json::json;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Backend {
    #[default]
    Auto,
    Rust,
}

#[derive(Clone, Debug, Parser)]
#[command(name = "slimevr-gui", about = "SlimeVR Tauri interface")]
pub struct LaunchOptions {
    /// Connect to an existing backend without starting a child process.
    #[arg(long)]
    pub no_server: bool,
    /// Diagnostic verbosity for the frontend, desktop and owned Rust backend.
    #[arg(long, value_enum)]
    pub log_level: Option<LogLevel>,
    /// Start the Rust backend, or connect to an existing service in auto mode.
    #[arg(long, value_enum, default_value_t=Backend::Auto)]
    pub backend: Backend,
    /// Explicit Rust server executable.
    #[arg(long)]
    pub rust_server: Option<PathBuf>,
    /// Original SlimeVR YAML, default: the server config folder/vrconfig.yml.
    #[arg(long = "config", alias = "rust-state")]
    pub rust_state: Option<PathBuf>,
    /// Disable the Rust SteamVR bridge.
    #[arg(long)]
    pub no_steamvr: bool,
    /// Override the Rust SteamVR local IPC endpoint.
    #[arg(long, conflicts_with = "no_steamvr")]
    pub steamvr_endpoint: Option<PathBuf>,
    /// Original SlimeVR bindings provider executable.
    #[arg(long, conflicts_with = "no_bindings_provider")]
    pub bindings_provider: Option<PathBuf>,
    #[arg(long)]
    pub no_bindings_provider: bool,
    /// Seed a new Rust backend state with a pose configuration.
    #[arg(long)]
    pub pose_config: Option<PathBuf>,
    /// Directory containing the Rust server executable.
    #[arg(short, long)]
    pub path: Option<PathBuf>,
    #[arg(short, long)]
    pub steam: bool,
}

#[derive(Default)]
pub struct ServerProcess {
    child: Mutex<Option<Child>>,
    history: Mutex<std::collections::VecDeque<ServerStatus>>,
    stopping: AtomicBool,
}

#[derive(Clone, Serialize)]
pub struct ServerStatus {
    #[serde(rename = "type")]
    kind: String,
    message: String,
    level: LogLevel,
}

impl ServerProcess {
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            if let Some(mut process) = child.take() {
                stop_child(&mut process);
            }
        }
    }
}

fn stop_child(process: &mut Child) {
    if let Some(input) = process.stdin.take() {
        // Rust owns this pipe and finalizes BVH/journals on EOF on every platform.
        drop(input);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if matches!(process.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    let _ = process.kill();
    let _ = process.wait();
}

fn emit_status(app: &AppHandle, kind: &str, message: impl Into<String>) {
    let message = message.into();
    let logger = app.state::<Logger>();
    let level = status_level(kind, &message);
    if !logger.enabled(level) {
        return;
    }
    let status = ServerStatus {
        kind: kind.into(),
        message,
        level,
    };
    let state = app.state::<ServerProcess>();
    if let Ok(mut history) = state.history.lock() {
        if history.len() >= 200 {
            history.pop_front();
        }
        history.push_back(status.clone());
    }
    let source = if matches!(kind, "stdout" | "stderr") {
        "backend"
    } else {
        "desktop"
    };
    let _ = logger.append(level, source, &[json!(status.message)]);
    let _ = app.emit("server-status", status);
}

#[tauri::command]
pub fn server_status_history(state: tauri::State<'_, ServerProcess>) -> Vec<ServerStatus> {
    state
        .history
        .lock()
        .map(|h| h.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn find_rust_server(
    options: &LaunchOptions,
    paths: &AppPaths,
) -> Result<Option<PathBuf>, String> {
    let filename = if cfg!(windows) {
        "slimevr-server.exe"
    } else {
        "slimevr-server"
    };
    let explicit = options.rust_server.clone().or_else(|| {
        options
            .path
            .as_ref()
            .map(|directory| directory.join(filename))
    });
    if let Some(path) = explicit {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !path.is_file() {
            return Err("Rust server executable is not a file".into());
        }
        return Ok(Some(path));
    }
    let mut candidates = vec![
        paths.resources.join(filename),
        paths.executable.join(filename),
    ];
    #[cfg(debug_assertions)]
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../server-rust/target");
        candidates.push(root.join("release").join(filename));
        candidates.push(root.join("debug").join(filename));
    }
    Ok(candidates
        .into_iter()
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok()))
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let options = app.state::<LaunchOptions>();
        if options.no_server {
            return;
        }
        if std::net::TcpStream::connect_timeout(
            &"127.0.0.1:21110".parse().unwrap(),
            Duration::from_millis(250),
        )
        .is_ok()
        {
            emit_status(
                &app,
                "other",
                "Port 21110 is already in use; connecting without starting another server.",
            );
            return;
        }
        let paths = app.state::<AppPaths>();
        let result = (|| -> Result<(), String> {
            let rust = find_rust_server(&options, &paths)?;
            let (mut command, label) = if let Some(binary) = rust {
                let mut command = Command::new(&binary);
                let state = options.rust_state.clone().unwrap_or_else(|| {
                    let yml = paths.server.join("vrconfig.yml");
                    if !yml.exists() && yml.with_extension("yaml").exists() {
                        yml.with_extension("yaml")
                    } else {
                        yml
                    }
                });
                command
                    .args([
                        "listen",
                        "--pose-output-ms",
                        "1000",
                        "--shutdown-on-stdin-eof",
                        "--api-bind",
                        "127.0.0.1:21110",
                        "--config",
                    ])
                    .arg(state)
                    .arg("--log-level")
                    .arg(app.state::<Logger>().level().as_str());
                if let Some(pose) = &options.pose_config {
                    command.arg("--pose-config").arg(pose);
                }
                let driver_name = if cfg!(windows) {
                    "slimevr-openvr-driver-win64"
                } else if cfg!(target_arch = "aarch64") {
                    "slimevr-openvr-driver-aarch64-linux"
                } else {
                    "slimevr-openvr-driver-x64-linux"
                };
                for root in [
                    paths.resources.join("drivers"),
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/drivers"),
                ] {
                    let driver = root.join(driver_name);
                    if driver.join("driver.vrdrivermanifest").is_file() {
                        command.arg("--steamvr-driver").arg(driver);
                        break;
                    }
                }
                if options.no_steamvr {
                    command.arg("--no-steamvr");
                }
                if let Some(endpoint) = &options.steamvr_endpoint {
                    command.arg("--steamvr-endpoint").arg(endpoint);
                }
                if let Some(provider) = &options.bindings_provider {
                    command.arg("--bindings-provider").arg(provider);
                } else {
                    let name = if cfg!(windows) {
                        "SlimeVR-Bindings-Provider.exe"
                    } else {
                        "slimevr-bindings-provider"
                    };
                    let platform = if cfg!(target_os = "windows") {
                        "win64"
                    } else if cfg!(target_arch = "aarch64") {
                        "linuxarm64"
                    } else {
                        "linux64"
                    };
                    let bundled = paths.resources.join("bindings").join(platform).join(name);
                    let provider = if bundled.is_file() {
                        bundled
                    } else {
                        paths.resources.join(name)
                    };
                    if provider.is_file() && !options.no_bindings_provider {
                        command.arg("--bindings-provider").arg(provider);
                    }
                }
                if options.no_bindings_provider {
                    command.arg("--no-bindings-provider");
                }
                command.current_dir(&paths.server).stdin(Stdio::piped());
                (command, format!("Rust server: {}", binary.display()))
            } else {
                if options.backend == Backend::Rust {
                    return Err(
                        "No Rust server found. Build server-rust or pass --rust-server.".into(),
                    );
                }
                emit_status(
                    &app,
                    "other",
                    "No Rust backend found. Start a server separately, or pass --rust-server.",
                );
                return Ok(());
            };
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            hide_console(&mut command);
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            for (kind, stream) in [
                (
                    "stdout",
                    child
                        .stdout
                        .take()
                        .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
                ),
                (
                    "stderr",
                    child
                        .stderr
                        .take()
                        .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
                ),
            ] {
                if let Some(stream) = stream {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(stream).lines().map_while(Result::ok) {
                            emit_status(&app, kind, line);
                        }
                    });
                }
            }
            let state = app.state::<ServerProcess>();
            let mut owned = state.child.lock().map_err(|e| e.to_string())?;
            if state.stopping.load(Ordering::SeqCst) {
                stop_child(&mut child);
                return Ok(());
            }
            *owned = Some(child);
            drop(owned);
            emit_status(&app, "other", format!("Started {label}"));
            let app = app.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(250));
                let state = app.state::<ServerProcess>();
                let Ok(mut child) = state.child.lock() else {
                    break;
                };
                let Some(process) = child.as_mut() else { break };
                match process.try_wait() {
                    Ok(Some(status)) => {
                        child.take();
                        drop(child);
                        emit_status(&app, "terminated", format!("Backend exited: {status}"));
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        drop(child);
                        emit_status(&app, "error", error.to_string());
                        break;
                    }
                }
            });
            Ok(())
        })();
        if let Err(error) = result {
            emit_status(&app, "error", error);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_options_select_rust_without_requiring_java() {
        let options = LaunchOptions::try_parse_from([
            "slimevr-gui",
            "--backend",
            "rust",
            "--rust-server",
            "server",
            "--config",
            "vrconfig.yml",
            "--steamvr-endpoint",
            "custom-endpoint",
            "--no-bindings-provider",
        ])
        .unwrap();
        assert_eq!(options.backend, Backend::Rust);
        assert_eq!(options.rust_state.unwrap(), PathBuf::from("vrconfig.yml"));
        assert_eq!(
            options.steamvr_endpoint.unwrap(),
            PathBuf::from("custom-endpoint")
        );
        assert!(options.no_bindings_provider);
        assert!(
            LaunchOptions::try_parse_from(["slimevr-gui", "--rust-state", "legacy.json"]).is_ok()
        );
        assert!(LaunchOptions::try_parse_from([
            "slimevr-gui",
            "--rust-server",
            "server",
            "--server-jar",
            "server.jar"
        ])
        .is_err());
        assert_eq!(
            LaunchOptions::try_parse_from(["slimevr-gui", "--no-server"])
                .unwrap()
                .backend,
            Backend::Auto
        );
    }
    #[test]
    fn rejects_removed_java_launch_options() {
        for args in [
            vec!["slimevr-gui", "--backend", "java"],
            vec!["slimevr-gui", "--server-jar", "server.jar"],
            vec!["slimevr-gui", "--java-path", "java"],
        ] {
            assert!(LaunchOptions::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn explicit_rust_directory_takes_priority_over_bundled_backend() {
        let root = std::env::temp_dir().join(format!(
            "slimevr-tauri-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let resources = root.join("resources");
        let explicit = root.join("custom");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::create_dir_all(&explicit).unwrap();
        let filename = if cfg!(windows) {
            "slimevr-server.exe"
        } else {
            "slimevr-server"
        };
        std::fs::write(resources.join(filename), b"bundled").unwrap();
        std::fs::write(resources.join("slimevr.jar"), b"legacy").unwrap();
        std::fs::write(explicit.join(filename), b"explicit").unwrap();
        let paths = AppPaths {
            gui: root.clone(),
            server: root.clone(),
            logs: root.clone(),
            resources,
            executable: root,
        };
        let options =
            LaunchOptions::try_parse_from(["slimevr-gui", "--path", explicit.to_str().unwrap()])
                .unwrap();
        assert_eq!(
            find_rust_server(&options, &paths).unwrap(),
            Some(explicit.join(filename).canonicalize().unwrap())
        );
        std::fs::remove_file(explicit.join(filename)).unwrap();
        assert!(find_rust_server(&options, &paths).is_err());
    }
}
