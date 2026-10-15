//! AutoBone's guided recording, confirmation and application lifecycle.
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    PutTrackersOn,
    Requirements,
    Preparation,
    StartRecording,
    Recording,
    VerifyResults,
    Done,
}
impl Step {
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn numbered(self) -> bool {
        !matches!(self, Self::Recording | Self::Done)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Resetting,
    Recording,
    Processing,
    Applying,
}

#[derive(Debug)]
struct Confirmation {
    session: u64,
    tx: u32,
    started: Instant,
}

#[derive(Debug, Default)]
pub struct Wizard {
    pub step: Step,
    pub phase: Phase,
    pub progress: f64,
    pub eta: f64,
    pub result: Option<Value>,
    pub failed: bool,
    pub demo_started: Option<Instant>,
    confirmation: Option<Confirmation>,
}
impl Wizard {
    pub fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }
    pub fn advance(&mut self) {
        if self.busy() {
            return;
        }
        self.step = match self.step {
            Step::PutTrackersOn => Step::Requirements,
            Step::Requirements => Step::Preparation,
            step => step,
        };
    }
    pub fn previous(&mut self) {
        if self.busy() {
            return;
        }
        self.demo_started = None;
        self.step = match self.step {
            Step::Requirements => Step::PutTrackersOn,
            Step::Preparation => Step::Requirements,
            Step::StartRecording => Step::Preparation,
            step => step,
        };
    }
    pub fn wait_for_reset(&mut self, session: u64, tx: u32) {
        self.phase = Phase::Resetting;
        self.confirmation = Some(Confirmation {
            session,
            tx,
            started: Instant::now(),
        });
    }
    pub fn reset_completed(&mut self, session: u64, tx: u32) {
        if self.phase == Phase::Resetting && self.matches(session, tx) {
            self.phase = Phase::Idle;
            self.confirmation = None;
            self.demo_started = None;
            self.step = Step::StartRecording;
        }
    }
    pub fn record(&mut self) {
        self.demo_started = Some(Instant::now());
        self.step = Step::Recording;
        self.phase = Phase::Recording;
        self.progress = 0.;
        self.eta = -1.;
        self.result = None;
        self.failed = false;
        self.confirmation = None;
    }
    /// Returns whether recording completion should start processing.
    pub fn status(&mut self, value: &Value) -> bool {
        let kind = match self.phase {
            Phase::Recording => 1,
            Phase::Processing => 3,
            _ => return false,
        };
        if value["process_type"].as_u64() != Some(kind) {
            return false;
        }
        self.eta = value["eta"].as_f64().unwrap_or(-1.);
        if let Some(total) = value["total"].as_f64().filter(|t| *t > 0.) {
            self.progress = (value["current"].as_f64().unwrap_or(0.) / total).clamp(0., 1.);
        }
        if value["completed"] != true {
            return false;
        }
        if value["success"] != true {
            self.fail();
            return false;
        }
        if self.phase == Phase::Recording {
            self.phase = Phase::Processing;
            self.progress = 0.;
            self.eta = -1.;
            return true;
        }
        if self.result.as_ref().is_some_and(|r| {
            r["adjusted_skeleton_parts"]
                .as_array()
                .is_some_and(|parts| !parts.is_empty())
        }) {
            self.demo_started = None;
            self.step = Step::VerifyResults;
            self.phase = Phase::Idle;
            self.progress = 1.;
        } else {
            self.fail();
        }
        false
    }
    pub fn epoch(&mut self, value: &Value) {
        if self.phase == Phase::Processing {
            if let Some(total) = value["total_epochs"].as_f64().filter(|t| *t > 0.) {
                self.progress =
                    (value["current_epoch"].as_f64().unwrap_or(0.) / total).clamp(0., 1.);
            }
            self.result = Some(value.clone());
        }
    }
    pub fn wait_for_apply(&mut self, session: u64, tx: u32) {
        self.phase = Phase::Applying;
        self.confirmation = Some(Confirmation {
            session,
            tx,
            started: Instant::now(),
        });
    }
    /// The matching skeleton response confirms the saved application batch.
    pub fn applied(&mut self, session: u64, tx: u32) -> bool {
        if self.phase == Phase::Applying && self.matches(session, tx) {
            self.step = Step::Done;
            self.phase = Phase::Idle;
            self.confirmation = None;
            return true;
        }
        false
    }
    fn matches(&self, session: u64, tx: u32) -> bool {
        self.confirmation
            .as_ref()
            .is_some_and(|c| c.session == session && c.tx == tx)
    }
    pub fn check_confirmation(&mut self, now: Instant, backend_error: bool) {
        if backend_error && self.busy()
            || self
                .confirmation
                .as_ref()
                .is_some_and(|c| now.saturating_duration_since(c.started) > Duration::from_secs(75))
        {
            self.fail();
        }
    }
    pub fn fail(&mut self) {
        self.demo_started = None;
        self.failed = true;
        self.phase = Phase::Idle;
        self.confirmation = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status(kind: u64, success: bool) -> Value {
        json!({"process_type":kind,"completed":true,"success":success})
    }
    fn calibrated() -> Wizard {
        let mut w = Wizard::default();
        w.record();
        assert!(w.status(&status(1, true)));
        w.epoch(&json!({"adjusted_skeleton_parts":[{"bone":3,"value":0.25}]}));
        w.status(&status(3, true));
        w
    }
    #[test]
    fn preparation_waits_for_its_reset_session_and_transaction() {
        let mut w = Wizard::default();
        w.advance();
        w.advance();
        w.advance();
        assert_eq!(w.step, Step::Preparation);
        w.wait_for_reset(2, 123);
        w.reset_completed(1, 123);
        w.reset_completed(2, 124);
        assert_eq!(w.step, Step::Preparation);
        w.reset_completed(2, 123);
        assert_eq!(w.step, Step::StartRecording);
    }
    #[test]
    fn recording_processes_once_and_success_requires_results() {
        let mut w = Wizard::default();
        w.record();
        assert!(!w.status(&status(3, true)));
        assert!(w.status(&status(1, true)));
        assert!(!w.status(&status(1, true)));
        w.status(&status(3, true));
        assert!(w.failed);
        assert_eq!(w.step, Step::Recording);
        assert_eq!(calibrated().step, Step::VerifyResults);
    }
    #[test]
    fn application_completes_after_matching_persisted_response() {
        let mut w = calibrated();
        w.wait_for_apply(2, 150);
        assert!(!w.applied(1, 150));
        assert!(!w.applied(2, 151));
        assert_eq!(w.step, Step::VerifyResults);
        assert!(w.applied(2, 150));
        assert_eq!(w.step, Step::Done);
        assert!(!w.applied(2, 150));
    }
    #[test]
    fn redo_retires_result_and_failure_recovers_to_first_step() {
        let mut w = calibrated();
        w.record();
        assert!(w.result.is_none());
        w.status(&status(1, false));
        assert!(w.failed);
        w = Wizard::default();
        assert_eq!(w.step, Step::PutTrackersOn);
        w.epoch(&json!({"adjusted_skeleton_parts":[1]}));
        w.status(&status(3, true));
        assert!(w.result.is_none());
    }
    #[test]
    fn confirmation_timeout_and_errors_keep_completion_pending() {
        let mut w = calibrated();
        w.wait_for_apply(1, 42);
        w.check_confirmation(Instant::now() + Duration::from_secs(76), false);
        assert!(w.failed);
        assert_eq!(w.step, Step::VerifyResults);
        let mut w = Wizard::default();
        w.wait_for_reset(1, 43);
        w.check_confirmation(Instant::now(), true);
        assert!(w.failed);
        let mut w = Wizard::default();
        w.record();
        w.check_confirmation(Instant::now(), true);
        assert!(w.failed);
    }
}
