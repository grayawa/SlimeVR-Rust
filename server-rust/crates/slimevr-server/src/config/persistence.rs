//! Atomic synchronous persistence primitive, called by the runtime's disk worker.
use super::{error, to_yaml, FrontendConfig, MAX_CONFIG_BYTES};
use crate::{log_level::LogLevel, logging};
use serde::Serialize;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Default, Serialize)]
pub(crate) struct SaveReport {
    #[serde(rename = "type")]
    kind: &'static str,
    outcome: &'static str,
    total_ms: f64,
    validation_ms: f64,
    serialization_ms: f64,
    file_io_ms: f64,
    // Subset of file_io_ms, not an additional stage.
    sync_all_ms: f64,
    serialized_bytes: usize,
    files_written: u64,
    sync_all_calls: u64,
    error_kind: Option<String>,
}
impl SaveReport {
    pub(super) fn emit_async(&self, revision: u64, queue_delay: std::time::Duration) {
        #[derive(Serialize)]
        struct AsyncReport<'a> {
            #[serde(flatten)]
            report: &'a SaveReport,
            revision: u64,
            queue_delay_ms: f64,
            background: bool,
        }
        logging::diagnostic(
            if self.outcome == "error" || self.total_ms > 4. {
                LogLevel::Warn
            } else {
                LogLevel::Info
            },
            &AsyncReport {
                report: self,
                revision,
                queue_delay_ms: queue_delay.as_secs_f64() * 1000.,
                background: true,
            },
        );
    }
    pub(crate) fn emit(&self) {
        let level = if self.outcome == "error" || self.total_ms > 4. {
            LogLevel::Warn
        } else {
            LogLevel::Info
        };
        logging::diagnostic(level, self);
    }
}

pub fn save(c: &FrontendConfig, path: Option<&Path>) -> io::Result<()> {
    let (result, report) = save_measured(c, path);
    if let Some(report) = report {
        report.emit();
    }
    result
}

// Startup defers emission until after listening, preserving the readiness event.
pub(crate) fn save_measured(
    c: &FrontendConfig,
    path: Option<&Path>,
) -> (io::Result<()>, Option<SaveReport>) {
    let Some(path) = path else {
        return (c.validate().map_err(error), None);
    };
    let started = Instant::now();
    let mut report = SaveReport {
        kind: "config_save_timing",
        outcome: "saved",
        ..Default::default()
    };
    let result = (|| {
        let phase = Instant::now();
        let validation = c.validate().map_err(error);
        report.validation_ms = phase.elapsed().as_secs_f64() * 1000.;
        validation?;
        let phase = Instant::now();
        let serialized =
            (|| serde_yaml_ng::to_string(&to_yaml(c)?).map_err(|e| error(e.to_string())))();
        report.serialization_ms = phase.elapsed().as_secs_f64() * 1000.;
        let data = serialized?;
        report.serialized_bytes = data.len();
        if data.len() as u64 > MAX_CONFIG_BYTES {
            return Err(error("serialized configuration exceeds 8 MiB"));
        }
        let phase = Instant::now();
        let persisted = (|| {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(parent)?;
            if path.exists() {
                let original = fs::read(path)?;
                if original == data.as_bytes() {
                    report.outcome = "unchanged";
                    return Ok(());
                }
                let backup = PathBuf::from(format!("{}.bak", path.display()));
                atomic_write(parent, &backup, &original, &mut report)?;
            }
            atomic_write(parent, path, data.as_bytes(), &mut report)
        })();
        report.file_io_ms = phase.elapsed().as_secs_f64() * 1000.;
        persisted
    })();
    report.total_ms = started.elapsed().as_secs_f64() * 1000.;
    if let Err(error) = &result {
        report.outcome = "error";
        report.error_kind = Some(format!("{:?}", error.kind()));
    }
    (result, Some(report))
}

fn atomic_write(
    parent: &Path,
    path: &Path,
    data: &[u8],
    report: &mut SaveReport,
) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(data)?;
    let phase = Instant::now();
    report.sync_all_calls += 1;
    let synced = file.as_file().sync_all();
    report.sync_all_ms += phase.elapsed().as_secs_f64() * 1000.;
    synced?;
    file.persist(path).map_err(|e| e.error)?;
    report.files_written += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_reports_backup_sync_and_noop_without_changing_persistence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vrconfig.yml");
        let mut config = FrontendConfig::default();
        let (result, report) = save_measured(&config, Some(&path));
        result.unwrap();
        let report = report.unwrap();
        assert_eq!(report.outcome, "saved");
        assert_eq!((report.files_written, report.sync_all_calls), (1, 1));
        let original = fs::read(&path).unwrap();
        let (result, report) = save_measured(&config, Some(&path));
        result.unwrap();
        let report = report.unwrap();
        assert_eq!(report.outcome, "unchanged");
        assert_eq!((report.files_written, report.sync_all_calls), (0, 0));
        assert!(!path.with_extension("yml.bak").exists());
        config.pose.skeleton.hips_width += 0.01;
        let (result, report) = save_measured(&config, Some(&path));
        result.unwrap();
        let report = report.unwrap();
        assert_eq!((report.files_written, report.sync_all_calls), (2, 2));
        assert_eq!(fs::read(path.with_extension("yml.bak")).unwrap(), original);
        assert!(report.sync_all_ms <= report.file_io_ms);
        assert_eq!(
            serde_json::to_value(report).unwrap()["type"],
            "config_save_timing"
        );
    }
    #[test]
    fn validation_and_io_errors_report_the_failed_stage_and_keep_memory_only_saves_quiet() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("blocked");
        fs::write(&path, b"keep").unwrap();
        let mut config = FrontendConfig::default();
        let (result, report) = save_measured(&config, Some(&path.join("vrconfig.yml")));
        assert!(result.is_err());
        let report = report.unwrap();
        assert_eq!(report.outcome, "error");
        assert!(report.error_kind.is_some());
        assert!(report.serialized_bytes > 0);
        assert_eq!(report.sync_all_calls, 0);
        assert_eq!(fs::read(&path).unwrap(), b"keep");
        config.pose.skeleton.hips_width = -1.;
        let (result, report) = save_measured(&config, Some(&path));
        assert!(result.is_err());
        let report = report.unwrap();
        assert_eq!(report.serialized_bytes, 0);
        assert_eq!(report.file_io_ms, 0.);
        let (result, report) = save_measured(&config, None);
        assert!(result.is_err());
        assert!(report.is_none());
    }
}
