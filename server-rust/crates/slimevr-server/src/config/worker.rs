//! One disk writer; at most one in-flight and one latest pending configuration.
use crate::api::FrontendConfig;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

struct Job {
    revision: u64,
    config: Arc<FrontendConfig>,
    queued: Instant,
}
pub(crate) struct Completion {
    pub revision: u64,
    pub result: Result<(), String>,
    pub previous_error: Option<String>,
}
struct State {
    pending: Option<Job>,
    completion: Option<Completion>,
    last_result: Result<(), String>,
    closing: bool,
    failure: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            pending: None,
            completion: None,
            last_result: Ok(()),
            closing: false,
            failure: None,
        }
    }
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    ready: AtomicBool,
}
pub(crate) struct Writer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    revision: u64,
}
impl Writer {
    pub(crate) fn start(path: PathBuf) -> Result<Self, String> {
        Self::with_write(move |job| {
            let delay = job.queued.elapsed();
            let (result, report) = super::persistence::save_measured(&job.config, Some(&path));
            if let Some(report) = report {
                report.emit_async(job.revision, delay);
            }
            result.map_err(|error| format!("Unable to save SlimeVR configuration: {error}"))
        })
    }
    fn with_write(
        mut write: impl FnMut(&Job) -> Result<(), String> + Send + 'static,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        let incoming = shared.clone();
        let thread = thread::Builder::new()
            .name("slimevr-config-writer".into())
            .spawn(move || {
                loop {
                    let job = {
                        let mut state = incoming.state.lock().unwrap_or_else(|e| e.into_inner());
                        while state.pending.is_none() && !state.closing {
                            state = incoming.wake.wait(state).unwrap_or_else(|e| e.into_inner());
                        }
                        let Some(job) = state.pending.take() else {
                            break;
                        };
                        job
                    };
                    // Disk and YAML work never hold the mailbox lock.
                    let result = write(&job);
                    let mut state = incoming.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.last_result = result.clone();
                    if let Err(error) = &result {
                        state.failure = Some(error.clone());
                    }
                    state.completion = Some(Completion {
                        revision: job.revision,
                        result,
                        previous_error: None,
                    });
                    incoming.ready.store(true, Ordering::Release);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            thread: Some(thread),
            revision: 0,
        })
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn submit(&mut self, config: Arc<FrontendConfig>) -> Result<(), String> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closing {
            return Err("configuration writer is closing".into());
        }
        self.revision += 1;
        state.pending = Some(Job {
            revision: self.revision,
            config,
            queued: Instant::now(),
        });
        drop(state);
        self.shared.wake.notify_one();
        Ok(())
    }
    pub(crate) fn poll(&self) -> Option<Completion> {
        if !self.shared.ready.swap(false, Ordering::Acquire) {
            return None;
        }
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut completion = state.completion.take()?;
        completion.previous_error = state.failure.take();
        Some(completion)
    }
    fn close(&self) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .closing = true;
        self.shared.wake.notify_one();
    }
    pub(crate) async fn finish(&mut self) -> Result<(), String> {
        self.close();
        if let Some(thread) = self.thread.take() {
            tokio::task::spawn_blocking(move || thread.join())
                .await
                .map_err(|e| e.to_string())?
                .map_err(|_| "configuration writer panicked".to_owned())?;
        }
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last_result
            .clone()
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.close();
        // Runtime explicitly awaits finish after ticks stop. This fallback covers
        // early errors and library users; it never participates in a normal tick.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn blocked_disk_keeps_only_latest_pending_and_shutdown_drains_it_in_order() {
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let revisions = Arc::new(Mutex::new(Vec::new()));
        let written = revisions.clone();
        let mut writer = Writer::with_write(move |job| {
            written
                .lock()
                .unwrap()
                .push((job.revision, job.config.sample_ms));
            if job.revision == 1 {
                entered
                    .send(thread::current().name().map(str::to_owned))
                    .unwrap();
                gate.recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            }
            Ok(())
        })
        .unwrap();
        writer.submit(Arc::new(FrontendConfig::default())).unwrap();
        assert_eq!(
            waiting
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                .as_deref(),
            Some("slimevr-config-writer")
        );
        // Producer and pose work progress while the fake disk is still blocked.
        let mut engine = slimevr_core::pose::PoseEngine::new(Default::default()).unwrap();
        for revision in 2..=100 {
            let config = FrontendConfig {
                sample_ms: revision,
                ..Default::default()
            };
            writer.submit(Arc::new(config)).unwrap();
            engine.tick(revision).unwrap();
        }
        assert_eq!(engine.snapshot().frame, 99);
        assert_eq!(
            writer
                .shared
                .state
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .revision,
            100
        );
        assert!(writer.poll().is_none());
        release.send(()).unwrap();
        writer.finish().await.unwrap();
        assert_eq!(*revisions.lock().unwrap(), [(1, 20), (100, 100)]);
        let completed = writer.poll().unwrap();
        assert_eq!(completed.revision, 100);
        assert!(completed.result.is_ok());
        assert!(writer.submit(Arc::new(FrontendConfig::default())).is_err());
    }
    #[tokio::test]
    async fn errors_are_not_lost_when_a_newer_save_recovers_before_the_owner_polls() {
        let (failed, waiting) = std::sync::mpsc::channel();
        let mut writer = Writer::with_write(move |job| {
            if job.revision == 1 {
                failed.send(()).unwrap();
                Err("disk blocked".into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        writer.submit(Arc::new(FrontendConfig::default())).unwrap();
        waiting
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        writer.submit(Arc::new(FrontendConfig::default())).unwrap();
        writer.finish().await.unwrap();
        let completed = writer.poll().unwrap();
        assert_eq!(completed.revision, 2);
        assert!(completed.result.is_ok());
        assert_eq!(completed.previous_error.as_deref(), Some("disk blocked"));
        assert!(writer.poll().is_none());
    }
}
