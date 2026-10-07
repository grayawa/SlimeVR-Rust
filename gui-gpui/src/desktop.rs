//! Shared desktop paths and compatibility with existing Tauri GUI preferences.
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    time::Duration,
};
#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub logs: PathBuf,
    pub config: PathBuf,
    pub resources: PathBuf,
}
impl Paths {
    pub fn new() -> Result<Self, String> {
        let root = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        }
        .ok_or("Application data directory unavailable")?
        .join("dev.slimevr.SlimeVR");
        let logs = root.join("logs");
        std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
        let yml = root.join("vrconfig.yml");
        let config = if !yml.exists() && yml.with_extension("yaml").exists() {
            yml.with_extension("yaml")
        } else {
            yml
        };
        let resources = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("Executable path unavailable")?
            .to_path_buf();
        Ok(Self {
            root,
            logs,
            config,
            resources,
        })
    }
    pub fn documents(&self) -> PathBuf {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(|p| PathBuf::from(p).join("Documents"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| self.root.clone())
    }
}
pub struct Preferences {
    pub value: Value,
    pub path: PathBuf,
    store: Value,
}
impl Preferences {
    pub fn load(paths: &Paths) -> Result<Self, String> {
        let path = paths.root.join("settings.json");
        let store = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice::<Value>(&b)
                .map_err(|e| format!("Unable to read existing GUI preferences: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(e) => return Err(e.to_string()),
        };
        let existing = store
            .get("config.json")
            .map(|v| {
                if let Some(s) = v.as_str() {
                    serde_json::from_str::<Value>(s).map_err(|e| e.to_string())
                } else {
                    Ok(v.clone())
                }
            })
            .transpose()?
            .unwrap_or(json!({}));
        if !existing.is_object() || !store.is_object() {
            return Err("Existing GUI preferences must be an object".into());
        }
        let mut value = Self::defaults();
        for (k, v) in existing.as_object().unwrap() {
            value[k] = v.clone();
        }
        Ok(Self { value, path, store })
    }
    pub fn defaults() -> Value {
        json!({"uuid":uuid::Uuid::new_v4().to_string(),"lang":"zh-Hans","doneOnboarding":false,"watchNewDevices":true,"feedbackSound":true,"feedbackSoundVolume":0.5,"connectedTrackersWarning":true,"theme":"slime","textSize":12,"fonts":["poppins"],"useTray":false,"mirrorView":true,"discordPresence":false,"homeLayout":"default","skeletonPreview":true,"bvhDirectory":null,"devSettings":{"highContrast":false,"preciseRotation":false,"fastDataFeed":false,"filterSlimesAndHMD":false,"sortByName":false,"rawSlimeRotation":false,"moreInfo":false}})
    }
    pub fn reset_known(&mut self) -> Result<(), String> {
        fn merge(target: &mut Value, defaults: &Value) {
            if let Some(map) = defaults.as_object() {
                if !target.is_object() {
                    *target = json!({});
                }
                for (key, value) in map {
                    if !matches!(key.as_str(), "lang" | "uuid") {
                        merge(&mut target[key], value);
                    }
                }
            } else {
                *target = defaults.clone();
            }
        }
        merge(&mut self.value, &Self::defaults());
        self.save()
    }
    pub fn store_value(&self, key: &str) -> &Value {
        &self.store[key]
    }
    /// An add-on owns its store key, while config.json belongs to the GUI.
    /// Writing this key must preserve the GUI's latest serialized preferences.
    pub fn save_store_value(&mut self, key: &str, value: Value) -> Result<(), String> {
        self.store = match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(error) => return Err(error.to_string()),
        };
        if !self.store.is_object() {
            return Err("Existing GUI preference store must be an object".into());
        }
        self.store[key] = value;
        self.write_store()
    }
    pub fn save(&mut self) -> Result<(), String> {
        // Reload unrelated store keys, preserving changes made by the other frontend.
        if let Ok(bytes) = std::fs::read(&self.path)
            && let Ok(store) = serde_json::from_slice::<Value>(&bytes)
            && store.is_object()
        {
            self.store = store;
        }
        self.store["config.json"] =
            json!(serde_json::to_string(&self.value).map_err(|e| e.to_string())?);
        self.write_store()
    }
    fn write_store(&self) -> Result<(), String> {
        let temporary = self
            .path
            .with_extension(format!("gpui-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(&self.store).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let result = std::fs::rename(&temporary, &self.path).map_err(|e| e.to_string());
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}
pub struct SingleInstance {
    _lock: File,
    listener: TcpListener,
    address_path: PathBuf,
}
impl SingleInstance {
    pub fn acquire(paths: &Paths) -> Result<Option<Self>, String> {
        let address_path = paths.root.join("gui-gpui.instance-address");
        let mut lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(paths.root.join("gui-gpui.lock"))
            .map_err(|e| e.to_string())?;
        if let Err(error) = lock.try_lock_exclusive() {
            if error.kind() != std::io::ErrorKind::WouldBlock
                && error.raw_os_error() != fs2::lock_contended_error().raw_os_error()
            {
                return Err(error.to_string());
            }
            // Windows byte-range locks also block reads from the lock file.
            // Keep the wake-up address in an unlocked sidecar instead.
            let address = match std::fs::read_to_string(&address_path) {
                Ok(address) => address,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.to_string()),
            };
            if let Ok(address) = address.trim().parse()
                && let Ok(mut stream) =
                    TcpStream::connect_timeout(&address, Duration::from_millis(300))
            {
                let _ = stream.write_all(b"show");
            }
            return Ok(None);
        }
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        lock.set_len(0).map_err(|e| e.to_string())?;
        lock.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        write!(
            lock,
            "{}",
            listener.local_addr().map_err(|e| e.to_string())?
        )
        .map_err(|e| e.to_string())?;
        lock.flush().map_err(|e| e.to_string())?;
        std::fs::write(
            &address_path,
            listener
                .local_addr()
                .map_err(|e| e.to_string())?
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
        Ok(Some(Self {
            _lock: lock,
            listener,
            address_path,
        }))
    }
    pub fn requested(&self) -> bool {
        self.listener.accept().is_ok()
    }
}
impl Drop for SingleInstance {
    fn drop(&mut self) {
        // Remove the sidecar while the ownership lock is still held, so a new
        // instance cannot publish an address that this old instance deletes.
        let _ = std::fs::remove_file(&self.address_path);
    }
}

pub fn font_family(value: &Value) -> String {
    let font = value["fonts"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("System");
    match font.to_ascii_lowercase().as_str() {
        "poppins" => "Poppins",
        "noto-sans" | "noto sans" => "Noto Sans",
        "lexend" => "Lexend",
        "ubuntu" => "Ubuntu",
        "opendyslexic" | "open-dyslexic" => "OpenDyslexic",
        _ => "sans-serif",
    }
    .into()
}
