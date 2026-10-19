mod poses;
mod settings;
mod wizard;
use super::*;
use serde_json::{Value, json};
use slimevr_gpui::stay_aligned::{Action, Pose, Step, Wizard};
use slimevr_gpui::visualization::Camera;
use std::time::Instant;

#[derive(Default)]
pub(super) struct State {
    pub wizard: Wizard,
    pub modal: Option<Pose>,
    pub modal_pending: Option<(u64, u32, Action, Instant)>,
    pub manual: bool,
    retry: Option<Action>,
    scroll: ScrollHandle,
    camera_from: Camera,
    camera_started: Option<Instant>,
}

impl SlimeView {
    pub(super) fn stay_aligned_start(&mut self, cx: &mut Context<Self>) {
        self.stay_aligned = Default::default();
        self.stay_aligned_action(Action::Disable, cx);
    }
    fn stay_aligned_action(&mut self, action: Action, cx: &mut Context<Self>) {
        let result = match action {
            Action::FullReset => self.client.send(Command::Reset(ResetKind::Full)),
            Action::MountingReset => self.client.send(Command::Reset(ResetKind::Mounting)),
            _ => {
                let (name, args) = action.request();
                self.client.batch(vec![
                    (name.into(), args),
                    ("SettingsRequest".into(), json!({})),
                ])
            }
        };
        self.stay_aligned.retry = Some(action);
        match result {
            Ok(tx) => {
                self.ui_error = None;
                if self.stay_aligned.modal.is_some() {
                    self.stay_aligned.modal_pending =
                        Some((self.snapshot.session, tx, action, Instant::now()));
                } else {
                    self.stay_aligned
                        .wizard
                        .begin(self.snapshot.session, tx, action);
                }
            }
            Err(error) => {
                self.ui_error = Some(error);
                if self.stay_aligned.modal.is_none() {
                    self.stay_aligned.wizard.fail();
                }
            }
        }
        cx.notify();
    }
    pub(super) fn stay_aligned_confirm(
        &mut self,
        session: u64,
        tx: u32,
        value: &Value,
        cx: &mut Context<Self>,
    ) {
        let step = self.stay_aligned.wizard.step;
        let action = if let Some((s, t, action, _)) = self.stay_aligned.modal_pending {
            if (s, t) != (session, tx) {
                return;
            }
            self.stay_aligned.modal_pending = None;
            if action.confirms(value) {
                self.stay_aligned.modal = None;
                Some(action)
            } else {
                self.ui_error = Some(self.text("native-stay-aligned-save-error"));
                None
            }
        } else {
            self.stay_aligned
                .wizard
                .settings_response(session, tx, value)
        };
        if let Some(action) = action {
            for field in action.fields() {
                let path = format!("/stay_aligned/{field}");
                self.draft.dirty.remove(&path);
                self.draft.invalid.remove(&path);
            }
            self.ui_error = None;
            self.stay_aligned.retry = None;
            if action == Action::Disable {
                self.stay_aligned.scroll.set_offset(point(px(0.), px(-96.)));
            }
            if matches!(
                action,
                Action::Detect(Pose::Flat) | Action::Clear(Pose::Flat)
            ) && self.navigation.page == Page::StayAlignedSetup
            {
                self.stay_aligned_action(Action::Enable, cx);
            }
        }
        self.stay_aligned_step_changed(step, cx);
    }
    pub(super) fn stay_aligned_tick(
        &mut self,
        snapshot: &Snapshot,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        if snapshot.session != self.snapshot.session || snapshot.connection != Connection::Connected
        {
            if self.navigation.page == Page::StayAlignedSetup {
                self.stay_aligned.wizard = Default::default();
                self.stay_aligned.wizard.fail();
                self.stay_aligned.retry = Some(Action::Disable);
                self.stay_aligned.scroll.set_offset(point(px(0.), px(0.)));
            }
            self.stay_aligned.modal_pending = None;
            self.stay_aligned.manual = false;
            return;
        }
        let failed = self.stay_aligned.wizard.failed;
        self.stay_aligned.wizard.check_timeout(now);
        if self
            .stay_aligned
            .modal_pending
            .is_some_and(|p| now.saturating_duration_since(p.3) > Duration::from_secs(15))
        {
            self.stay_aligned.modal_pending = None;
            self.ui_error = Some(self.text("native-stay-aligned-save-error"));
            cx.notify();
        }
        if snapshot.last_error.is_some() && snapshot.last_error != self.snapshot.last_error {
            if self.stay_aligned.wizard.busy() {
                self.stay_aligned.wizard.fail();
            }
            self.stay_aligned.modal_pending = None;
        }
        if failed != self.stay_aligned.wizard.failed {
            cx.notify();
        }
        if self.navigation.page == Page::StayAlignedSetup
            && self
                .stay_aligned
                .camera_started
                .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(500))
        {
            cx.notify();
        }
    }
    fn stay_aligned_camera(&self) -> Camera {
        let target = match self.stay_aligned.wizard.step {
            Step::Standing => Camera {
                yaw: 0.,
                pitch: 0.,
                ..Default::default()
            },
            Step::Sitting | Step::Flat => Camera {
                yaw: std::f32::consts::FRAC_PI_2,
                pitch: 0.,
                ..Default::default()
            },
            _ => Camera {
                yaw: -0.78,
                pitch: 0.42,
                ..Default::default()
            },
        };
        let t = self
            .stay_aligned
            .camera_started
            .map(|at| (at.elapsed().as_secs_f32() / 0.5).clamp(0., 1.))
            .unwrap_or(1.);
        let t = if t < 0.5 {
            2. * t * t
        } else {
            1. - (-2. * t + 2.).powi(2) / 2.
        };
        let from = self.stay_aligned.camera_from;
        Camera {
            yaw: from.yaw + (target.yaw - from.yaw) * t,
            pitch: from.pitch + (target.pitch - from.pitch) * t,
            ..target
        }
    }
    pub(super) fn stay_aligned_step_changed(&mut self, previous: Step, cx: &mut Context<Self>) {
        if previous != self.stay_aligned.wizard.step {
            // The previous target seeds the transition for the next live camera view.
            let next = self.stay_aligned.wizard.step;
            self.stay_aligned.wizard.step = previous;
            self.stay_aligned.camera_from = self.stay_aligned_camera();
            self.stay_aligned.wizard.step = next;
            self.stay_aligned.camera_started = Some(Instant::now());
            self.stay_aligned
                .scroll
                .set_offset(point(px(0.), px(-(96. + (next as usize * 68) as f32))));
            cx.notify();
        }
    }
}
