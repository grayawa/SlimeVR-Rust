//! Stay Aligned setup, pose capture and confirmed settings transitions.
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Standing,
    Sitting,
    Flat,
}
impl Pose {
    pub const ALL: [Self; 3] = [Self::Standing, Self::Sitting, Self::Flat];
    pub fn wire(self) -> u8 {
        match self {
            Self::Standing => 0,
            Self::Sitting => 1,
            Self::Flat => 2,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Sitting => "sitting",
            Self::Flat => "flat",
        }
    }
    pub fn image(self) -> &'static str {
        match self {
            Self::Standing => "slime/stay-aligned/StayAlignedStanding.webp",
            Self::Sitting => "slime/stay-aligned/StayAlignedSitting.webp",
            Self::Flat => "slime/stay-aligned/StayAlignedFloor.webp",
        }
    }
    pub fn has_saved(self, settings: &Value) -> bool {
        settings[format!("{}Enabled", self.name())] == true
            || ["UpperLegAngle", "LowerLegAngle", "FootAngle"]
                .iter()
                .any(|field| {
                    settings[format!("{}{field}", self.name())]
                        .as_f64()
                        .is_some_and(|v| v != 0.)
                })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    PutTrackersOn,
    Preparation,
    VerifyMounting,
    Standing,
    Sitting,
    Flat,
    Done,
}
impl Step {
    pub const ALL: [Self; 7] = [
        Self::PutTrackersOn,
        Self::Preparation,
        Self::VerifyMounting,
        Self::Standing,
        Self::Sitting,
        Self::Flat,
        Self::Done,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::PutTrackersOn => "onboarding-stay_aligned-put_trackers_on-title",
            Self::Preparation => "onboarding-stay_aligned-preparation-title",
            Self::VerifyMounting => "onboarding-stay_aligned-verify_mounting-title",
            Self::Standing => "onboarding-stay_aligned-relaxed_poses-standing-title",
            Self::Sitting => "onboarding-stay_aligned-relaxed_poses-sitting-title",
            Self::Flat => "onboarding-stay_aligned-relaxed_poses-flat-title",
            Self::Done => "onboarding-stay_aligned-done-title",
        }
    }
    pub fn pose(self) -> Option<Pose> {
        match self {
            Self::Standing => Some(Pose::Standing),
            Self::Sitting => Some(Pose::Sitting),
            Self::Flat => Some(Pose::Flat),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Disable,
    Enable,
    FullReset,
    MountingReset,
    Detect(Pose),
    Clear(Pose),
}
impl Action {
    pub fn request(self) -> (&'static str, Value) {
        use serde_json::json;
        match self {
            Self::Disable => ("EnableStayAlignedRequest", json!({"enable":false})),
            Self::Enable => ("EnableStayAlignedRequest", json!({"enable":true})),
            Self::Detect(p) => (
                "DetectStayAlignedRelaxedPoseRequest",
                json!({"pose":p.wire()}),
            ),
            Self::Clear(p) => (
                "ResetStayAlignedRelaxedPoseRequest",
                json!({"pose":p.wire()}),
            ),
            Self::FullReset => ("ResetRequest", json!({"reset_type":1})),
            Self::MountingReset => ("ResetRequest", json!({"reset_type":2})),
        }
    }
    pub fn confirms(self, value: &Value) -> bool {
        let settings = &value["stay_aligned"];
        match self {
            Self::Disable => settings["enabled"] == false,
            Self::Enable => settings["enabled"] == true && settings["setupComplete"] == true,
            Self::Detect(p) => settings[format!("{}Enabled", p.name())] == true,
            Self::Clear(p) => {
                settings[format!("{}Enabled", p.name())] == false
                    && ["UpperLegAngle", "LowerLegAngle", "FootAngle"]
                        .iter()
                        .all(|field| {
                            settings[format!("{}{field}", p.name())]
                                .as_f64()
                                .unwrap_or(0.)
                                == 0.
                        })
            }
            _ => false,
        }
    }
    pub fn fields(self) -> Vec<String> {
        match self {
            Self::Disable | Self::Enable => vec!["enabled".into()],
            Self::Detect(p) | Self::Clear(p) => {
                ["Enabled", "UpperLegAngle", "LowerLegAngle", "FootAngle"]
                    .iter()
                    .map(|s| format!("{}{s}", p.name()))
                    .collect()
            }
            _ => vec![],
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Pending {
    session: u64,
    tx: u32,
    action: Action,
    at: Instant,
}
#[derive(Debug, Default)]
pub struct Wizard {
    pub step: Step,
    pub failed: bool,
    pending: Option<Pending>,
}
impl Wizard {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn begin(&mut self, session: u64, tx: u32, action: Action) {
        self.failed = false;
        self.pending = Some(Pending {
            session,
            tx,
            action,
            at: Instant::now(),
        });
    }
    pub fn advance(&mut self) {
        if !self.busy() && !self.failed && self.step == Step::PutTrackersOn {
            self.step = Step::Preparation;
        }
    }
    pub fn previous(&mut self) {
        if !self.busy() {
            self.failed = false;
            self.step = Step::ALL[(self.step as usize).saturating_sub(1)];
        }
    }
    pub fn manual_completed(&mut self) {
        if !self.busy() && self.step == Step::VerifyMounting {
            self.step = Step::Standing;
        }
    }
    pub fn reset_response(&mut self, session: u64, tx: u32, kind: u8, done: bool) {
        let Some(p) = self.pending else {
            return;
        };
        if (p.session, p.tx) != (session, tx) || !done {
            return;
        }
        match (p.action, kind, self.step) {
            (Action::FullReset, 1, Step::Preparation) => self.step = Step::VerifyMounting,
            (Action::MountingReset, 2, Step::VerifyMounting) => self.step = Step::Standing,
            _ => return,
        }
        self.pending = None;
    }
    pub fn settings_response(&mut self, session: u64, tx: u32, settings: &Value) -> Option<Action> {
        let p = self.pending?;
        if (p.session, p.tx) != (session, tx) {
            return None;
        }
        if matches!(p.action, Action::FullReset | Action::MountingReset) {
            return None;
        }
        let valid = p.action.confirms(settings);
        self.pending = None;
        self.failed = !valid;
        if !valid {
            return None;
        }
        match p.action {
            Action::Disable => self.step = Step::PutTrackersOn,
            Action::Enable => self.step = Step::Done,
            Action::Detect(p) | Action::Clear(p) => {
                self.step = match p {
                    Pose::Standing => Step::Sitting,
                    Pose::Sitting => Step::Flat,
                    Pose::Flat => Step::Flat,
                }
            }
            _ => {}
        }
        Some(p.action)
    }
    pub fn fail(&mut self) {
        self.pending = None;
        self.failed = true;
    }
    pub fn check_timeout(&mut self, now: Instant) {
        if self
            .pending
            .is_some_and(|p| now.saturating_duration_since(p.at) > Duration::from_secs(75))
        {
            self.fail();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn seven_steps_wait_for_matching_session_transaction_and_response_kind() {
        let mut w = Wizard::default();
        w.begin(2, 10, Action::Disable);
        assert!(
            w.settings_response(1, 10, &json!({"stay_aligned":{"enabled":false}}))
                .is_none()
        );
        assert!(
            w.settings_response(2, 0, &json!({"stay_aligned":{"enabled":false}}))
                .is_none()
        );
        w.settings_response(2, 10, &json!({"stay_aligned":{"enabled":false}}));
        w.advance();
        assert_eq!(w.step, Step::Preparation);
        w.begin(2, 11, Action::FullReset);
        w.reset_response(2, 11, 2, true);
        assert!(w.busy());
        w.reset_response(2, 11, 1, false);
        assert!(w.busy());
        w.reset_response(2, 11, 1, true);
        assert_eq!(w.step, Step::VerifyMounting);
        w.begin(2, 12, Action::MountingReset);
        w.reset_response(2, 12, 2, true);
        assert_eq!(w.step, Step::Standing);
        for (tx, pose) in Pose::ALL.into_iter().enumerate() {
            w.begin(2, 20 + tx as u32, Action::Detect(pose));
            w.settings_response(
                2,
                20 + tx as u32,
                &json!({"stay_aligned":{format!("{}Enabled",pose.name()):true}}),
            );
        }
        assert_eq!(w.step, Step::Flat);
        w.begin(2, 30, Action::Enable);
        w.settings_response(
            2,
            30,
            &json!({"stay_aligned":{"enabled":true,"setupComplete":true}}),
        );
        assert_eq!(w.step, Step::Done);
    }
    #[test]
    fn skips_clear_each_pose_and_failure_preserves_current_step_for_retry() {
        let mut w = Wizard {
            step: Step::Standing,
            ..Default::default()
        };
        w.begin(1, 1, Action::Detect(Pose::Standing));
        w.settings_response(1, 1, &json!({"stay_aligned":{"standingEnabled":false}}));
        assert!(w.failed);
        assert_eq!(w.step, Step::Standing);
        w.begin(1, 2, Action::Clear(Pose::Standing));
        w.settings_response(1, 2, &json!({"stay_aligned":{"standingEnabled":false}}));
        assert_eq!(w.step, Step::Sitting);
        w.begin(1, 3, Action::Detect(Pose::Sitting));
        w.check_timeout(Instant::now() + Duration::from_secs(76));
        assert!(w.failed);
        assert!(!w.busy());
        assert_eq!(w.step, Step::Sitting);
    }
    #[test]
    fn skipped_poses_clear_saved_values_and_completion_requires_setup_confirmation() {
        let mut wizard = Wizard {
            step: Step::Standing,
            ..Default::default()
        };
        for (tx, pose) in Pose::ALL.into_iter().enumerate() {
            let action = Action::Clear(pose);
            let (name, args) = action.request();
            assert_eq!(name, "ResetStayAlignedRelaxedPoseRequest");
            assert_eq!(args["pose"], pose.wire());
            wizard.begin(7, tx as u32, action);
            wizard.settings_response(
                7,
                tx as u32,
                &json!({"stay_aligned":{format!("{}Enabled",pose.name()):false}}),
            );
        }
        assert_eq!(wizard.step, Step::Flat);
        wizard.begin(7, 9, Action::Enable);
        wizard.settings_response(
            7,
            9,
            &json!({"stay_aligned":{"enabled":true,"setupComplete":false}}),
        );
        assert!(wizard.failed);
        assert_eq!(wizard.step, Step::Flat);
        wizard.begin(7, 10, Action::Enable);
        wizard.settings_response(
            7,
            10,
            &json!({"stay_aligned":{"enabled":true,"setupComplete":true}}),
        );
        assert_eq!(wizard.step, Step::Done);
    }
    #[test]
    fn pose_toggles_follow_saved_angles_and_capture_flags() {
        assert!(
            !Pose::Standing.has_saved(&json!({"standingEnabled":false,"standingUpperLegAngle":0.}))
        );
        assert!(Pose::Standing.has_saved(&json!({"standingEnabled":true})));
        assert!(Pose::Sitting.has_saved(&json!({"sittingLowerLegAngle":4.})));
    }
}
