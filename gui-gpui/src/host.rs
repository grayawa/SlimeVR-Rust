//! Optional owned backend; closing stdin requests graceful recording finalization.
use crate::log_level::LogLevel;
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub struct Backend(Child, Vec<std::thread::JoinHandle<()>>);
impl Backend {
    pub fn start(
        executable: &Path,
        config: Option<&Path>,
        url: &str,
        level: LogLevel,
    ) -> Result<Self, String> {
        let address = url
            .strip_prefix("ws://")
            .ok_or("Expected ws:// endpoint")?
            .trim_end_matches('/')
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "Owned backend requires ws://127.0.0.1:PORT or ws://[::1]:PORT")?;
        if !address.ip().is_loopback() {
            return Err("Owned backend must bind to a loopback address".into());
        }
        if std::net::TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
            return Err("The backend port is already in use. Omit --backend to connect to the existing service.".into());
        }
        let executable =
            std::fs::canonicalize(executable).map_err(|e| format!("Backend executable: {e}"))?;
        let config = config.map(|p| {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(p)
            }
        });
        let config = config.as_deref();
        let mut command = Command::new(&executable);
        command
            .args([
                "listen",
                "--api-bind",
                &address.to_string(),
                "--shutdown-on-stdin-eof",
                "--log-level",
                level.as_str(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(config) = config {
            command.arg("--config").arg(config);
        }
        let resources = executable.parent().unwrap_or_else(|| Path::new("."));
        let driver = resources.join("drivers").join(if cfg!(windows) {
            "slimevr-openvr-driver-win64"
        } else if cfg!(target_arch = "aarch64") {
            "slimevr-openvr-driver-aarch64-linux"
        } else {
            "slimevr-openvr-driver-x64-linux"
        });
        if driver.join("driver.vrdrivermanifest").is_file() {
            command.arg("--steamvr-driver").arg(driver);
        }
        let provider = resources
            .join("bindings")
            .join(if cfg!(windows) {
                "win64"
            } else if cfg!(target_arch = "aarch64") {
                "linuxarm64"
            } else {
                "linux64"
            })
            .join(if cfg!(windows) {
                "SlimeVR-Bindings-Provider.exe"
            } else {
                "slimevr-bindings-provider"
            });
        if provider.is_file() {
            command.arg("--bindings-provider").arg(provider);
        }
        command.args(["--pose-output-ms", "1000"]);
        if let Some(config) = config
            && let Some(parent) = config.parent()
        {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            command.current_dir(parent);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Unable to start backend: {e}"))?;
        let mut readers = Vec::new();
        fn pump(
            stream: impl std::io::Read + Send + 'static,
            source: &'static str,
            level: LogLevel,
        ) -> std::thread::JoinHandle<()> {
            std::thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stream).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    let event = serde_json::from_str::<serde_json::Value>(&line).ok();
                    let severity = event
                        .as_ref()
                        .and_then(|e| e["level"].as_str())
                        .and_then(|l| LogLevel::parse(l).ok())
                        .unwrap_or(if source == "backend-stderr" {
                            LogLevel::Warn
                        } else {
                            LogLevel::Info
                        });
                    if level.allows(severity) {
                        crate::logging::write(severity, source, &line);
                        eprintln!("[{source}] {line}");
                    }
                }
            })
        }
        if let Some(stdout) = child.stdout.take() {
            readers.push(pump(stdout, "backend", level));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(pump(stderr, "backend-stderr", level));
        }
        Ok(Self(child, readers))
    }
    pub fn failure(&mut self) -> Option<String> {
        match self.0.try_wait() {
            Ok(Some(status)) => Some(format!("Backend exited: {status}")),
            Err(e) => Some(e.to_string()),
            Ok(None) => None,
        }
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.0.try_wait() {
                Ok(Some(_)) => {
                    for reader in self.1.drain(..) {
                        let _ = reader.join();
                    }
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                _ => break,
            }
        }
        eprintln!("[warn] Backend did not exit within 10 seconds; terminating owned child");
        let _ = self.0.kill();
        let _ = self.0.wait();
        for reader in self.1.drain(..) {
            let _ = reader.join();
        }
    }
}
