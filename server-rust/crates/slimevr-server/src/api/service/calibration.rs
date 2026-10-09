//! Delayed resets and hotkey actions on the pose clock.
use super::Service;
use crate::api::protocol;
use serde_json::json;
use slimevr_core::{
    calibration::ResetKind,
    pose::{PoseEngine, SceneInput},
    skeleton::BodyPosition as B,
};
use solarxr_protocol as sx;
use solarxr_protocol::rpc;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct PendingReset {
    pub(super) at: u64,
    pub(super) start: u64,
    pub(super) kind: rpc::ResetType,
    pub(super) parts: Vec<solarxr_protocol::datatypes::BodyPart>,
    pub(super) tx: u32,
    pub(super) last_progress: u64,
}
impl Service {
    pub fn hotkey(
        &mut self,
        action: crate::hotkeys::Action,
        delay_ms: u64,
        at: u64,
    ) -> Result<(), String> {
        use crate::hotkeys::Action;
        let due = at.saturating_add(delay_ms.min(60000));
        if action == Action::Pause {
            if self.key_pauses.len() >= 8 {
                return Err("pause queue full".into());
            }
            self.key_pauses.push(due);
        } else {
            if self.resets.len() >= 8 {
                return Err("reset queue full".into());
            }
            let kind = match action {
                Action::Full => rpc::ResetType::Full,
                Action::Yaw => rpc::ResetType::Yaw,
                _ => rpc::ResetType::Mounting,
            };
            let parts = if action == Action::Feet {
                vec![
                    sx::datatypes::BodyPart::LEFT_FOOT,
                    sx::datatypes::BodyPart::RIGHT_FOOT,
                ]
            } else {
                vec![]
            };
            self.resets.push(PendingReset {
                at: due,
                start: at,
                kind,
                parts,
                tx: 0,
                last_progress: at,
            });
        }
        Ok(())
    }
    pub fn before_tick(&mut self, engine: &mut PoseEngine, at: u64) {
        let pauses = std::mem::take(&mut self.key_pauses);
        for due in pauses {
            if due <= at {
                if let Err(e) = self.steamvr_input(
                    SceneInput::Pause {
                        at_ms: at,
                        paused: !engine.is_paused(),
                    },
                    engine,
                ) {
                    self.error(e);
                }
            } else {
                self.key_pauses.push(due);
            }
        }

        let resets = std::mem::take(&mut self.resets);
        for mut reset in resets {
            if at >= reset.at {
                let mut bodies: BTreeSet<B> = reset
                    .parts
                    .iter()
                    .filter_map(|v| protocol::from_body(*v).ok())
                    .collect();
                let kind = reset_kind(reset.kind).unwrap();
                if reset.parts.is_empty() && matches!(kind, ResetKind::Mounting) {
                    bodies = engine.default_mounting_bodies();
                }
                match engine.reset_selected(at, kind, &bodies) {
                    Ok(()) => {
                        crate::logging::diagnostic(
                            crate::log_level::LogLevel::Info,
                            &json!({"type":"reset_complete", "at_ms":at, "kind":format!("{kind:?}"), "bodies":bodies}),
                        );
                        self.changes.push(SceneInput::ResetSelected {
                            at_ms: at,
                            kind,
                            bodies,
                        });
                        self.broadcast(protocol::reset_frame(
                            reset.tx,
                            reset.kind,
                            &reset.parts,
                            reset.at - reset.start,
                            reset.at - reset.start,
                            true,
                        ));
                    }
                    Err(e) => self.error(e),
                }
            } else {
                // Match ResetTimer.kt: send one progress notification per whole second.
                // The GUI uses each STARTED notification for its countdown cue.
                let progress = (at - reset.start) / 1000 * 1000;
                if progress > reset.last_progress - reset.start {
                    self.broadcast(protocol::reset_frame(
                        reset.tx,
                        reset.kind,
                        &reset.parts,
                        progress,
                        reset.at - reset.start,
                        false,
                    ));
                    reset.last_progress = reset.start + progress;
                }
                self.resets.push(reset);
            }
        }
    }
}
pub(super) fn reset_kind(kind: rpc::ResetType) -> Result<ResetKind, String> {
    match kind {
        rpc::ResetType::Full => Ok(ResetKind::Full),
        rpc::ResetType::Yaw => Ok(ResetKind::Yaw),
        rpc::ResetType::Mounting => Ok(ResetKind::Mounting),
        _ => Err("unknown reset type".into()),
    }
}
