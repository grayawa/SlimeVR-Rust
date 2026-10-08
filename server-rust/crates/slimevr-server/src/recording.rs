//! Versioned JSONL journal: inputs, clock ticks and expected receiver replies.
use crate::receiver::{Effects, Outbound, Receiver, ReceiverConfig};
use serde::{Deserialize, Serialize};
use slimevr_core::InputEvent;
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    net::SocketAddr,
    path::Path,
};
use thiserror::Error;

const MAX_LINE: usize = 256 * 1024;

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum Record {
    Hid {
        at_ms: u64,
        event: crate::hid::Event,
    },
    Header {
        version: u32,
        config: ReceiverConfig,
    },
    Receive {
        at_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        received_at_ms: Option<u64>,
        from: SocketAddr,
        hex: String,
    },
    Tick {
        at_ms: u64,
    },
    PoseSetup {
        at_ms: u64,
        config: Box<slimevr_core::pose::PoseConfig>,
    },
    Control {
        at_ms: u64,
        input: slimevr_core::pose::SceneInput,
    },
    ForgetDevice {
        at_ms: u64,
        device_key: String,
    },
    Admission {
        at_ms: u64,
        allowed_macs: Vec<String>,
    },
    DeviceConfig {
        at_ms: u64,
        command: crate::receiver::DeviceConfig,
    },
    Send {
        at_ms: u64,
        to: SocketAddr,
        hex: String,
    },
    End {
        at_ms: u64,
    },
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn decode_hex(hex: &str) -> Result<Vec<u8>, JournalError> {
    if hex.len() > 65536 * 2
        || !hex.len().is_multiple_of(2)
        || !hex.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(JournalError::Invalid(
            "invalid hex or payload exceeds 65536 bytes".into(),
        ));
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| JournalError::Invalid(e.to_string()))
        })
        .collect()
}

pub struct Recorder {
    writer: BufWriter<File>,
}
impl Recorder {
    pub fn create(path: &Path, config: &ReceiverConfig) -> Result<Self, JournalError> {
        Self::create_version(path, config, 1)
    }
    pub fn create_version(
        path: &Path,
        config: &ReceiverConfig,
        version: u32,
    ) -> Result<Self, JournalError> {
        if !matches!(version, 1..=4) {
            return Err(JournalError::Invalid("unsupported journal version".into()));
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut this = Self {
            writer: BufWriter::new(options.open(path)?),
        };
        this.write(&Record::Header {
            version,
            config: config.clone(),
        })?;
        Ok(this)
    }
    pub fn write(&mut self, record: &Record) -> Result<(), JournalError> {
        serde_json::to_writer(&mut self.writer, record)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), JournalError> {
        self.writer.flush()?;
        Ok(())
    }
    pub fn finish(&mut self, at_ms: u64) -> Result<(), JournalError> {
        self.write(&Record::End { at_ms })?;
        self.flush()
    }
}

pub struct ReplayResult {
    pub receiver: Receiver,
    pub at_ms: u64,
    pub records: usize,
    pub verified_replies: usize,
}

/// No sockets or wall-clock sleeps. Reject corrupt ordering, truncated journals and differing replies.
pub fn replay(
    path: &Path,
    mut event: impl FnMut(&InputEvent),
) -> Result<ReplayResult, JournalError> {
    replay_observed(path, |input| {
        if let ReplayInput::Event(e) = input {
            event(e);
        }
        Ok(())
    })
}

pub enum ReplayInput<'a> {
    Event(&'a InputEvent),
    Tick(u64),
    Control(&'a slimevr_core::pose::SceneInput),
    PoseSetup(&'a slimevr_core::pose::PoseConfig),
}

/// Observer sees events before the corresponding solve tick. It may fail without panicking.
pub fn replay_observed(
    path: &Path,
    mut observe: impl FnMut(ReplayInput<'_>) -> Result<(), String>,
) -> Result<ReplayResult, JournalError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut receiver: Option<Receiver> = None;
    let mut expected: VecDeque<(u64, Outbound)> = VecDeque::new();
    let mut at_ms = 0;
    let mut records = 0;
    let mut verified_replies = 0;
    let mut ended = false;
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take((MAX_LINE + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if ended {
            return Err(JournalError::Invalid("records after journal end".into()));
        }
        records += 1;
        if count > MAX_LINE || line.last() != Some(&b'\n') {
            return Err(JournalError::Invalid(format!(
                "line {records}: oversized or truncated record"
            )));
        }
        let record: Record = serde_json::from_slice(&line)
            .map_err(|e| JournalError::Invalid(format!("line {records}: {e}")))?;
        if let Record::Header { version, config } = record {
            if records != 1 || !matches!(version, 1..=4) {
                return Err(JournalError::Invalid(
                    "invalid header or journal version".into(),
                ));
            }
            receiver = Some(Receiver::new(config).map_err(JournalError::Invalid)?);
            continue;
        }
        let r = receiver
            .as_mut()
            .ok_or_else(|| JournalError::Invalid("header must be the first record".into()))?;
        let next_at = match &record {
            Record::Hid { at_ms, .. }
            | Record::Receive { at_ms, .. }
            | Record::Tick { at_ms }
            | Record::PoseSetup { at_ms, .. }
            | Record::Control { at_ms, .. }
            | Record::Admission { at_ms, .. }
            | Record::ForgetDevice { at_ms, .. }
            | Record::DeviceConfig { at_ms, .. }
            | Record::Send { at_ms, .. }
            | Record::End { at_ms } => *at_ms,
            _ => unreachable!(),
        };
        if next_at < at_ms {
            return Err(JournalError::Invalid(format!(
                "line {records}: clock went backwards"
            )));
        }
        at_ms = next_at;
        let is_tick = matches!(record, Record::Tick { .. });
        let effects: Effects = match record {
            Record::DeviceConfig { command, .. } => {
                r.config_command(&command).map_err(JournalError::Invalid)?
            }
            Record::Hid { event, .. } => r.hid(&event, at_ms),
            Record::Receive {
                from,
                hex,
                received_at_ms,
                ..
            } => {
                if !expected.is_empty() {
                    return Err(JournalError::Invalid(format!(
                        "line {records}: missing reply record"
                    )));
                }
                if received_at_ms.is_some_and(|received| received > at_ms) {
                    return Err(JournalError::Invalid(
                        "UDP receive clock is after processing clock".into(),
                    ));
                }
                r.receive_timed(from, &decode_hex(&hex)?, at_ms, received_at_ms)
            }
            Record::PoseSetup { config, .. } => {
                if records != 2 || at_ms != 0 {
                    return Err(JournalError::Invalid(
                        "pose setup must follow the header".into(),
                    ));
                }
                config.validate().map_err(JournalError::Invalid)?;
                observe(ReplayInput::PoseSetup(&config)).map_err(JournalError::Invalid)?;
                continue;
            }
            Record::Control { input, .. } => {
                if !expected.is_empty() {
                    return Err(JournalError::Invalid(
                        "control appears before expected replies".into(),
                    ));
                }
                if let slimevr_core::pose::SceneInput::Input { event } = &input {
                    r.external(event).map_err(JournalError::Invalid)?;
                }
                observe(ReplayInput::Control(&input)).map_err(JournalError::Invalid)?;
                continue;
            }
            Record::ForgetDevice { device_key, .. } => {
                if !expected.is_empty() {
                    return Err(JournalError::Invalid(
                        "forget appears before expected replies".into(),
                    ));
                }
                r.forget_device(&device_key);
                continue;
            }
            Record::Admission { allowed_macs, .. } => {
                if !expected.is_empty() {
                    return Err(JournalError::Invalid(
                        "admission appears before expected replies".into(),
                    ));
                }
                let mut config = r.config.clone();
                config.allowed_macs = allowed_macs;
                config.validate().map_err(JournalError::Invalid)?;
                r.config = config;
                continue;
            }
            Record::Tick { .. } => {
                if !expected.is_empty() {
                    return Err(JournalError::Invalid(format!(
                        "line {records}: missing reply record"
                    )));
                }
                r.tick(at_ms)
            }
            Record::Send { to, hex, .. } => {
                let actual = (
                    at_ms,
                    Outbound {
                        to,
                        bytes: decode_hex(&hex)?,
                    },
                );
                if expected.pop_front().as_ref() != Some(&actual) {
                    return Err(JournalError::Invalid(format!(
                        "line {records}: reply differs from receiver output"
                    )));
                }
                verified_replies += 1;
                continue;
            }
            Record::Header { .. } => unreachable!(),
            Record::End { .. } => {
                ended = true;
                continue;
            }
        };
        for e in &effects.events {
            observe(ReplayInput::Event(e)).map_err(JournalError::Invalid)?;
        }
        if is_tick {
            observe(ReplayInput::Tick(at_ms)).map_err(JournalError::Invalid)?;
        }
        expected.extend(effects.outbound.into_iter().map(|p| (at_ms, p)));
    }
    if !expected.is_empty() {
        return Err(JournalError::Invalid(
            "journal ended before expected replies".into(),
        ));
    }
    if !ended {
        return Err(JournalError::Invalid(
            "incomplete journal: missing end record".into(),
        ));
    }
    let receiver = receiver.ok_or_else(|| JournalError::Invalid("empty journal".into()))?;
    Ok(ReplayResult {
        receiver,
        at_ms,
        records,
        verified_replies,
    })
}
