//! Driver output diagnostics. Never wait for the writer or alter session flow control.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct OutputCounters {
    pub steamvr_output_batches_enqueued: u64,
    pub steamvr_output_queue_full: u64,
    pub steamvr_output_queue_closed: u64,
    pub steamvr_output_batches_written: u64,
    pub steamvr_output_write_failed: u64,
    pub steamvr_output_write_cancelled: u64,
    pub steamvr_output_stale_batches: u64,
}

#[derive(Default)]
pub struct OutputStats {
    pub(super) enqueued: AtomicU64,
    pub(super) queue_full: AtomicU64,
    pub(super) queue_closed: AtomicU64,
    pub(super) written: AtomicU64,
    pub(super) write_failed: AtomicU64,
    pub(super) write_cancelled: AtomicU64,
    pub(super) stale_batches: AtomicU64,
}
impl OutputStats {
    pub fn snapshot(&self) -> OutputCounters {
        self.read(false)
    }
    pub(crate) fn take_window(&self) -> OutputCounters {
        self.read(true)
    }
    fn read(&self, reset: bool) -> OutputCounters {
        let read = |counter: &AtomicU64| {
            if reset {
                counter.swap(0, Ordering::Relaxed)
            } else {
                counter.load(Ordering::Relaxed)
            }
        };
        OutputCounters {
            steamvr_output_batches_enqueued: read(&self.enqueued),
            steamvr_output_queue_full: read(&self.queue_full),
            steamvr_output_queue_closed: read(&self.queue_closed),
            steamvr_output_batches_written: read(&self.written),
            steamvr_output_write_failed: read(&self.write_failed),
            steamvr_output_write_cancelled: read(&self.write_cancelled),
            steamvr_output_stale_batches: read(&self.stale_batches),
        }
    }
}

// Dropping the connection's send future can interrupt an in-progress write.
// Count that separately from a completed write and an explicit I/O error.
pub(super) struct WriteBatch<'a> {
    stats: &'a OutputStats,
    finished: bool,
}
impl<'a> WriteBatch<'a> {
    pub fn new(stats: &'a OutputStats) -> Self {
        Self {
            stats,
            finished: false,
        }
    }
    pub fn finish(mut self, success: bool) {
        let counter = if success {
            &self.stats.written
        } else {
            &self.stats.write_failed
        };
        counter.fetch_add(1, Ordering::Relaxed);
        self.finished = true;
    }
}
impl Drop for WriteBatch<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.stats.write_cancelled.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_take_counts_once_without_inventing_completed_writes() {
        let stats = OutputStats::default();
        stats.enqueued.fetch_add(4, Ordering::Relaxed);
        stats.queue_full.fetch_add(2, Ordering::Relaxed);
        WriteBatch::new(&stats).finish(true);
        WriteBatch::new(&stats).finish(false);
        drop(WriteBatch::new(&stats));
        let report = stats.take_window();
        assert_eq!(report.steamvr_output_batches_enqueued, 4);
        assert_eq!(report.steamvr_output_queue_full, 2);
        assert_eq!(report.steamvr_output_batches_written, 1);
        assert_eq!(report.steamvr_output_write_failed, 1);
        assert_eq!(report.steamvr_output_write_cancelled, 1);
        assert_eq!(stats.snapshot(), OutputCounters::default());
        assert_eq!(stats.take_window(), OutputCounters::default());
    }
}
