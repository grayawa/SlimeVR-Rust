use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub const CONFIG_IDENTIFIER: &str = "dev.slimevr.SlimeVR";

#[derive(Clone)]
pub struct AppPaths {
    pub gui: PathBuf,
    pub server: PathBuf,
    pub logs: PathBuf,
    pub resources: PathBuf,
    pub executable: PathBuf,
}

impl AppPaths {
    pub fn new(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let gui = app.path().app_data_dir()?;
        let server = app.path().config_dir()?.join(CONFIG_IDENTIFIER);
        let logs = gui.join("logs");
        for dir in [&gui, &server, &logs] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(Self {
            gui,
            server,
            logs,
            resources: app.path().resource_dir()?,
            executable: std::env::current_exe()?
                .parent()
                .ok_or("Executable has no parent directory")?
                .to_path_buf(),
        })
    }

    pub fn allows_open(&self, path: &std::path::Path) -> bool {
        let Ok(path) = path.canonicalize() else {
            return false;
        };
        [&self.gui, &self.server, &self.logs]
            .iter()
            .any(|root| root.canonicalize().is_ok_and(|root| path.starts_with(root)))
    }
}
