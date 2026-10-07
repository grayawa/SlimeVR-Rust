//! One synchronized writer for frontend, desktop lifecycle and backend diagnostics.
use crate::log_level::LogLevel;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

const MAX_BYTES: u64 = 10 * 1024 * 1024;
const ARCHIVES: usize = 4;

pub struct Logger {
    level: LogLevel,
    writer: Mutex<Writer>,
}
struct Writer {
    directory: PathBuf,
    file: Option<BufWriter<File>>,
    bytes: u64,
    max_bytes: u64,
}
impl Logger {
    pub fn new(directory: PathBuf, level: LogLevel) -> Self {
        Self {
            level,
            writer: Mutex::new(Writer {
                directory,
                file: None,
                bytes: 0,
                max_bytes: MAX_BYTES,
            }),
        }
    }
    pub fn level(&self) -> LogLevel {
        self.level
    }
    pub fn enabled(&self, level: LogLevel) -> bool {
        self.level.allows(level)
    }
    pub fn append(&self, level: LogLevel, source: &str, args: &[Value]) -> Result<(), String> {
        if !self.enabled(level) {
            return Ok(());
        }
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        let mut line = serde_json::to_vec(&json!({"time":time / 1000, "time_ms":time, "level":level, "source":source, "args":args})).map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.writer
            .lock()
            .map_err(|e| e.to_string())?
            .append(&line)
            .map_err(|e| e.to_string())
    }
}
impl Writer {
    fn path(&self, index: usize) -> PathBuf {
        self.directory.join(if index == 0 {
            "gui-tauri.log".into()
        } else {
            format!("gui-tauri.{index}.log")
        })
    }
    fn append(&mut self, line: &[u8]) -> std::io::Result<()> {
        if self.file.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.path(0))?;
            self.bytes = file.metadata()?.len();
            self.file = Some(BufWriter::new(file));
        }
        if self.bytes > 0 && self.bytes.saturating_add(line.len() as u64) > self.max_bytes {
            if let Some(mut file) = self.file.take() {
                file.flush()?;
            }
            // Close the handle before renaming: Windows forbids renaming an open file.
            match std::fs::remove_file(self.path(ARCHIVES)) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
            for index in (0..ARCHIVES).rev() {
                match std::fs::rename(self.path(index), self.path(index + 1)) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }
            }
            self.file = Some(BufWriter::new(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.path(0))?,
            ));
            self.bytes = 0;
        }
        let file = self.file.as_mut().unwrap();
        file.write_all(line)?;
        file.flush()?; // Keep crash diagnostics readable immediately.
        self.bytes += line.len() as u64;
        Ok(())
    }
}

/// Stream names are transport details, not severities. Honor typed backend levels;
/// retain Java/plain-text stderr as warnings and fatal process failures as errors.
pub fn status_level(kind: &str, message: &str) -> LogLevel {
    match kind {
        "error" | "terminated" => LogLevel::Error,
        "stdout" | "stderr" => {
            if let Ok(value) = serde_json::from_str::<Value>(message) {
                if let Some(level) = value
                    .get("level")
                    .and_then(Value::as_str)
                    .and_then(|s| LogLevel::parse(s).ok())
                {
                    return level;
                }
                match value.get("type").and_then(Value::as_str) {
                    Some("pose_snapshot" | "snapshot") => return LogLevel::Debug,
                    Some("sample" | "flex_value" | "telemetry" | "ignored") => {
                        return LogLevel::Trace
                    }
                    Some(
                        "api_connection_error" | "steamvr_error" | "discovery_error" | "send_error",
                    ) => return LogLevel::Warn,
                    Some("backend_error") => return LogLevel::Error,
                    _ => (),
                }
            }
            for (marker, level) in [
                ("[ERROR]", LogLevel::Error),
                ("[WARN]", LogLevel::Warn),
                ("[INFO]", LogLevel::Info),
                ("[DEBUG]", LogLevel::Debug),
                ("[TRACE]", LogLevel::Trace),
            ] {
                if message.contains(marker) {
                    return level;
                }
            }
            if kind == "stderr" {
                LogLevel::Warn
            } else {
                LogLevel::Info
            }
        }
        _ => LogLevel::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "slimevr-logs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn severity_does_not_confuse_stderr_or_stream_names_with_errors() {
        assert_eq!(
            status_level(
                "stderr",
                r#"{"type":"api_connection_error","level":"warn"}"#
            ),
            LogLevel::Warn
        );
        assert_eq!(
            status_level("stdout", r#"{"type":"pose_snapshot"}"#),
            LogLevel::Debug
        );
        assert_eq!(
            status_level("stdout", r#"{"type":"sample"}"#),
            LogLevel::Trace
        );
        assert_eq!(
            status_level("stderr", "[INFO] Java started"),
            LogLevel::Info
        );
        assert_eq!(status_level("error", "spawn failed"), LogLevel::Error);
        assert_eq!(status_level("terminated", "exit code 1"), LogLevel::Error);
    }
    #[test]
    fn filters_before_opening_and_keeps_warnings_with_source_and_millisecond_time() {
        let path = directory();
        let logger = Logger::new(path.clone(), LogLevel::Info);
        logger
            .append(LogLevel::Debug, "backend", &[json!("pose")])
            .unwrap();
        assert!(!path.join("gui-tauri.log").exists());
        logger
            .append(LogLevel::Warn, "backend", &[json!("timeout")])
            .unwrap();
        let line: Value =
            serde_json::from_str(&std::fs::read_to_string(path.join("gui-tauri.log")).unwrap())
                .unwrap();
        assert_eq!(line["level"], "warn");
        assert_eq!(line["source"], "backend");
        assert_eq!(
            line["time"].as_u64().unwrap(),
            line["time_ms"].as_u64().unwrap() / 1000
        );
        drop(logger);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn rotation_and_concurrent_writes_keep_complete_records_and_newest_files() {
        let path = directory();
        let logger = Logger::new(path.clone(), LogLevel::Info);
        logger.writer.lock().unwrap().max_bytes = 300;
        let logger = Arc::new(logger);
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let logger = logger.clone();
                std::thread::spawn(move || {
                    for index in 0..12 {
                        logger
                            .append(LogLevel::Info, "gui", &[json!([worker, index])])
                            .unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        logger
            .append(LogLevel::Error, "desktop", &[json!("final crash")])
            .unwrap();
        drop(logger);
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), ARCHIVES + 1);
        for entry in std::fs::read_dir(&path).unwrap() {
            let contents = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            assert!(contents.ends_with('\n'));
            for line in contents.lines() {
                let _: Value = serde_json::from_str(line).unwrap();
            }
        }
        assert!(std::fs::read_to_string(path.join("gui-tauri.log"))
            .unwrap()
            .contains("final crash"));
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn oversized_existing_log_rotates_on_first_new_record() {
        let path = directory();
        std::fs::write(path.join("gui-tauri.log"), "old log".repeat(100)).unwrap();
        let logger = Logger::new(path.clone(), LogLevel::Info);
        logger.writer.lock().unwrap().max_bytes = 300;
        logger
            .append(LogLevel::Info, "desktop", &[json!("startup")])
            .unwrap();
        assert!(path.join("gui-tauri.1.log").exists());
        assert!(!std::fs::read_to_string(path.join("gui-tauri.log"))
            .unwrap()
            .contains("old log"));
        drop(logger);
        std::fs::remove_dir_all(path).unwrap();
    }
}
