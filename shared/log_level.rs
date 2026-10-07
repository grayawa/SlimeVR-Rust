//! Severity shared by the backend and desktop host; independent of a logging framework.
use clap::ValueEnum;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn allows(self, level: Self) -> bool {
        level <= self
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        <Self as ValueEnum>::from_str(value, true)
    }
    pub fn resolve(explicit: Option<Self>, legacy_events: bool) -> Result<Self, String> {
        if let Some(level) = explicit {
            return Ok(level);
        }
        match std::env::var("SLIMEVR_LOG_LEVEL") {
            Ok(value) => Self::parse(&value),
            Err(std::env::VarError::NotPresent) => Ok(if legacy_events {
                Self::Trace
            } else {
                Self::Info
            }),
            Err(error) => Err(error.to_string()),
        }
    }
}
