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
    path::{Path, PathBuf},
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
    Java,
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
    /// Select the backend. Auto prefers an available Rust server, then Java.
    #[arg(long, value_enum, default_value_t=Backend::Auto)]
    pub backend: Backend,
    /// Explicit Rust server executable.
    #[arg(long, conflicts_with = "server_jar")]
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
    /// Directory containing slimevr.jar (compatible with the Electron --path option).
    #[arg(short, long)]
    pub path: Option<PathBuf>,
    /// Explicit path to the Java server JAR.
    #[arg(long)]
    pub server_jar: Option<PathBuf>,
    /// Explicit Java executable, otherwise use bundled JRE, JAVA_HOME or PATH.
    #[arg(long)]
    pub java_path: Option<PathBuf>,
    #[arg(short, long)]
    pub steam: bool,
    #[arg(short, long)]
    pub install: bool,
    #[arg(long)]
    pub no_udev: bool,
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

pub fn find_server_jar(
    options: &LaunchOptions,
    paths: &AppPaths,
) -> Result<Option<PathBuf>, String> {
    if let Some(explicit) = &options.server_jar {
        return explicit
            .canonicalize()
            .map(Some)
            .map_err(|e| format!("{}: {e}", explicit.display()));
    }
    let mut candidates = Vec::new();
    if let Some(dir) = &options.path {
        return dir
            .join("slimevr.jar")
            .canonicalize()
            .map(Some)
            .map_err(|e| format!("No slimevr.jar in {}: {e}", dir.display()));
    }
    candidates.extend([
        paths.resources.join("slimevr.jar"),
        paths.executable.join("slimevr.jar"),
        PathBuf::from("/usr/share/slimevr/slimevr.jar"),
        PathBuf::from("/app/share/slimevr/slimevr.jar"),
    ]);
    if cfg!(debug_assertions) {
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../server/desktop/build/libs/slimevr.jar"),
        );
    }
    Ok(candidates
        .into_iter()
        .find(|path| path.is_file())
        .and_then(|p| p.canonicalize().ok()))
}

fn java_major_version(output: &str) -> Option<u32> {
    let version = output.lines().find_map(|line| line.split('"').nth(1))?;
    let mut components = version.split(['.', '-', '+']);
    let first: u32 = components.next()?.parse().ok()?;
    if first == 1 {
        components.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn compatible_java(path: &Path) -> bool {
    let mut command = Command::new(path);
    command.arg("-version").stdin(Stdio::null());
    hide_console(&mut command);
    command.output().ok().is_some_and(|output| {
        output.status.success()
            && java_major_version(&format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            ))
            .is_some_and(|major| major >= 17)
    })
}

fn find_java(options: &LaunchOptions, jar: &Path, paths: &AppPaths) -> Result<PathBuf, String> {
    if let Some(explicit) = &options.java_path {
        return if compatible_java(explicit) {
            Ok(explicit.clone())
        } else {
            Err(format!("Java 17+ is required: {}", explicit.display()))
        };
    }
    let binary = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = vec![
        jar.parent()
            .unwrap_or(Path::new("."))
            .join("jre/bin")
            .join(binary),
        paths.resources.join("jre/bin").join(binary),
        paths.resources.join("jre/Contents/Home/bin").join(binary),
    ];
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join(binary));
    }
    for directory in ["/Library/Java/JavaVirtualMachines", "/usr/lib/jvm"] {
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                candidates.push(
                    entry
                        .path()
                        .join(if cfg!(target_os = "macos") {
                            "Contents/Home/bin"
                        } else {
                            "bin"
                        })
                        .join(binary),
                );
            }
        }
    }
    candidates.push(PathBuf::from(binary));
    candidates
        .into_iter()
        .find(|path| compatible_java(path))
        .ok_or_else(|| "Unable to find Java 17+. Set JAVA_HOME or pass --java-path.".into())
}

pub fn find_rust_server(
    options: &LaunchOptions,
    paths: &AppPaths,
) -> Result<Option<PathBuf>, String> {
    if let Some(path) = &options.rust_server {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !path.is_file() {
            return Err("Rust server executable is not a file".into());
        }
        return Ok(Some(path));
    }
    let filename = if cfg!(windows) {
        "slimevr-server.exe"
    } else {
        "slimevr-server"
    };
    let mut candidates = vec![
        paths.resources.join(filename),
        paths.executable.join(filename),
    ];
    if let Some(path) = &options.path {
        candidates.push(path.join(filename));
    }
    #[cfg(debug_assertions)]
    {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../server-rust/target");
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
            if options.backend == Backend::Java && options.rust_server.is_some() {
                return Err("--rust-server cannot be combined with --backend java".into());
            }
            if options.backend == Backend::Rust && options.server_jar.is_some() {
                return Err("--server-jar cannot be combined with --backend rust".into());
            }
            let rust = if options.backend == Backend::Java || options.server_jar.is_some() {
                None
            } else {
                find_rust_server(&options, &paths)?
            };
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
                let Some(jar) = find_server_jar(&options, &paths)? else {
                    emit_status(
                    &app,
                    "other",
                    "No backend found. Start a server separately, or pass --rust-server / --server-jar.",
                );
                    return Ok(());
                };
                let java = find_java(&options, &jar, &paths)?;
                let mut command = Command::new(java);
                command.args(["-Xmx128M", "-jar"]).arg(&jar);
                if options.steam {
                    command.arg("--steam");
                }
                if options.install {
                    command.arg("--install");
                }
                if options.no_udev {
                    command.arg("--no-udev");
                }
                command
                    .arg("run")
                    .current_dir(jar.parent().unwrap())
                    .stdin(Stdio::null());
                (command, format!("Java server: {}", jar.display()))
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
    fn parses_java_versions_without_treating_legacy_java_as_supported() {
        assert_eq!(
            java_major_version("openjdk version \"17.0.10\" 2024-01-16"),
            Some(17)
        );
        assert_eq!(java_major_version("java version \"1.8.0_401\""), Some(8));
        assert_eq!(java_major_version("openjdk version \"21-ea\""), Some(21));
        assert_eq!(java_major_version("not a Java version"), None);
    }
}
