//! Live diagnostics only. Replay/solve/decode and flash-worker output are machine data.
use crate::log_level::LogLevel;
use serde::Serialize;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicU8, Ordering},
};

static MAX_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);
pub fn configure(level: LogLevel) {
    MAX_LEVEL.store(level as u8, Ordering::Relaxed);
}
pub fn enabled(level: LogLevel) -> bool {
    level as u8 <= MAX_LEVEL.load(Ordering::Relaxed)
}

pub fn write_json(
    level: LogLevel,
    value: &impl Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    if !enabled(level) {
        return Ok(());
    }
    #[derive(Serialize)]
    struct Entry<'a, T: Serialize> {
        level: LogLevel,
        #[serde(flatten)]
        data: &'a T,
    }
    let entry = Entry { level, data: value };
    // Keeping warnings/errors on stderr also makes standalone runs easy to inspect.
    fn write(
        out: &mut impl Write,
        value: &impl Serialize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        serde_json::to_writer(&mut *out, value)?;
        writeln!(out)?;
        Ok(())
    }
    if level <= LogLevel::Warn {
        write(&mut io::stderr().lock(), &entry)
    } else {
        write(&mut io::stdout().lock(), &entry)
    }
}

/// A failed diagnostic write must not terminate an API client or an operation.
pub fn diagnostic(level: LogLevel, value: &impl Serialize) {
    let _ = write_json(level, value);
}

pub fn event_level(kind: &slimevr_core::EventKind) -> LogLevel {
    use slimevr_core::{EventKind as E, SensorStatus};
    match kind {
        E::Sample { .. } | E::FlexValue { .. } | E::Telemetry { .. } | E::Ignored { .. } => {
            LogLevel::Trace
        }
        E::Rejected { .. }
        | E::CompatibilityFallback { .. }
        | E::TransportState {
            timed_out: true, ..
        } => LogLevel::Warn,
        E::SensorState {
            status: SensorStatus::Error,
            ..
        } => LogLevel::Warn,
        _ => LogLevel::Info,
    }
}
