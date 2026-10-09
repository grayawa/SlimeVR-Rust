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

// Diagnostic output uses bounded entry and byte capacities with overflow drops.
// Replay journals and command output follow their own delivery contracts.
const QUEUE_ENTRIES: usize = 512;
const QUEUE_BYTES: usize = 4 * 1024 * 1024;
static WRITER: std::sync::OnceLock<Result<std::sync::Arc<Queue>, String>> =
    std::sync::OnceLock::new();

struct Line {
    level: LogLevel,
    bytes: Vec<u8>,
}
struct Queue {
    sender: std::sync::mpsc::SyncSender<Line>,
    pending_bytes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
    closing: std::sync::Arc<std::sync::atomic::AtomicBool>,
    done: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    budget: usize,
}
impl Queue {
    fn start(
        capacity: usize,
        budget: usize,
        mut write: impl FnMut(LogLevel, &[u8]) -> io::Result<()> + Send + 'static,
    ) -> io::Result<std::sync::Arc<Self>> {
        use std::sync::{
            atomic::{AtomicBool, AtomicU64, AtomicUsize},
            mpsc, Arc,
        };
        let (sender, receiver) = mpsc::sync_channel::<Line>(capacity);
        let (done, completed) = mpsc::channel();
        let pending_bytes = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicU64::new(0));
        let closing = Arc::new(AtomicBool::new(false));
        let queue = Arc::new(Self {
            sender,
            pending_bytes: pending_bytes.clone(),
            dropped: dropped.clone(),
            closing: closing.clone(),
            done: std::sync::Mutex::new(completed),
            budget,
        });
        std::thread::Builder::new()
            .name("slimevr-log-writer".into())
            .spawn(move || {
                let mut next_report = std::time::Instant::now();
                loop {
                    let line = if closing.load(Ordering::Acquire) {
                        receiver.try_recv().ok()
                    } else {
                        match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
                            Ok(line) => Some(line),
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => None,
                        }
                    };
                    let Some(line) = line else { break };
                    let _ = write(line.level, &line.bytes);
                    pending_bytes.fetch_sub(line.bytes.len(), Ordering::Relaxed);
                    if std::time::Instant::now() >= next_report {
                        report_dropped(&dropped, &mut write);
                        next_report = std::time::Instant::now() + std::time::Duration::from_secs(1);
                    }
                }
                report_dropped(&dropped, &mut write);
                let _ = done.send(());
            })?;
        Ok(queue)
    }
    fn enqueue(&self, level: LogLevel, bytes: Vec<u8>) {
        let size = bytes.len();
        if self.closing.load(Ordering::Acquire) || !self.reserve_bytes(size) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if self.sender.try_send(Line { level, bytes }).is_err() {
            self.pending_bytes.fetch_sub(size, Ordering::Relaxed);
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn reserve_bytes(&self, size: usize) -> bool {
        let mut current = self.pending_bytes.load(Ordering::Relaxed);
        loop {
            let Some(next) = current
                .checked_add(size)
                .filter(|next| *next <= self.budget)
            else {
                return false;
            };
            match self.pending_bytes.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }
    fn shutdown(&self, timeout: std::time::Duration) {
        self.closing.store(true, Ordering::Release);
        if let Ok(done) = self.done.lock() {
            let _ = done.recv_timeout(timeout);
        }
    }
}
fn report_dropped(
    dropped: &std::sync::atomic::AtomicU64,
    write: &mut impl FnMut(LogLevel, &[u8]) -> io::Result<()>,
) {
    let count = dropped.swap(0, Ordering::Relaxed);
    if count != 0 {
        let message = format!(
            "{{\"level\":\"warn\",\"type\":\"logging_backpressure\",\"dropped\":{count}}}\n"
        );
        let _ = write(LogLevel::Warn, message.as_bytes());
    }
}
fn queue() -> Result<&'static std::sync::Arc<Queue>, io::Error> {
    WRITER
        .get_or_init(|| {
            Queue::start(QUEUE_ENTRIES, QUEUE_BYTES, |level, bytes| {
                if level <= LogLevel::Warn {
                    io::stderr().lock().write_all(bytes)
                } else {
                    io::stdout().lock().write_all(bytes)
                }
            })
            .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| io::Error::other(error.clone()))
}
/// Keep this guard until all live diagnostics have been submitted. Shutdown drains
/// queued entries, with a two-second shutdown limit for a blocked parent pipe.
pub struct DrainGuard;
pub fn start() -> io::Result<DrainGuard> {
    queue()?;
    Ok(DrainGuard)
}
impl Drop for DrainGuard {
    fn drop(&mut self) {
        if let Ok(queue) = queue() {
            queue.shutdown(std::time::Duration::from_secs(2));
        }
    }
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
    let mut bytes = serde_json::to_vec(&Entry { level, data: value })?;
    bytes.push(b'\n');
    queue()?.enqueue(level, bytes);
    Ok(())
}

/// Submit best-effort diagnostics while preserving the caller's operation lifecycle.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc, Mutex},
        time::{Duration, Instant},
    };

    #[test]
    fn blocked_output_does_not_block_producer_or_exceed_queue_budget() {
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let output = lines.clone();
        let mut first = true;
        let queue = Queue::start(2, 12, move |level, bytes| {
            if first {
                first = false;
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            output.lock().unwrap().push((level, bytes.to_vec()));
            Ok(())
        })
        .unwrap();
        queue.enqueue(LogLevel::Info, b"one\n".to_vec());
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = Instant::now();
        queue.enqueue(LogLevel::Warn, b"two\n".to_vec());
        queue.enqueue(LogLevel::Debug, b"tri\n".to_vec());
        for _ in 0..1000 {
            queue.enqueue(LogLevel::Info, b"big\n".to_vec());
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(queue.pending_bytes.load(Ordering::Relaxed), 12);
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1000);
        // Shutdown has a deadline even if the consumer remains stuck in write().
        queue.shutdown(Duration::from_millis(10));
        release.send(()).unwrap();
        queue.shutdown(Duration::from_secs(5));
        assert_eq!(queue.pending_bytes.load(Ordering::Relaxed), 0);
        let lines = lines.lock().unwrap();
        let payloads: Vec<_> = lines.iter().filter(|(_, b)| b.len() == 4).collect();
        assert_eq!(payloads.len(), 3);
        assert_eq!(payloads[1].0, LogLevel::Warn);
        let reports: Vec<serde_json::Value> = lines
            .iter()
            .filter_map(|(_, b)| serde_json::from_slice(b).ok())
            .collect();
        assert_eq!(
            reports
                .iter()
                .map(|v| v["dropped"].as_u64().unwrap())
                .sum::<u64>(),
            1000
        );
    }

    #[test]
    fn oversized_diagnostics_and_full_entry_queue_are_accounted_separately() {
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut first = true;
        let queue = Queue::start(1, 1024, move |_, _| {
            if first {
                first = false;
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            Ok(())
        })
        .unwrap();
        queue.enqueue(LogLevel::Info, vec![0; 4]);
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        queue.enqueue(LogLevel::Info, vec![0; 4]);
        queue.enqueue(LogLevel::Info, vec![0; 4]); // entry limit
        queue.enqueue(LogLevel::Info, vec![0; 2048]); // byte limit
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 2);
        assert_eq!(queue.pending_bytes.load(Ordering::Relaxed), 8);
        release.send(()).unwrap();
        queue.shutdown(Duration::from_secs(5));
        assert_eq!(queue.pending_bytes.load(Ordering::Relaxed), 0);
    }
}
