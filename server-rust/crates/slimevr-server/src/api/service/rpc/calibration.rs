//! Reset, pause, alignment, leg and height requests.
use super::super::{
    calibration::{reset_kind, PendingReset},
    Service,
};
use crate::{
    api::{
        protocol::{self, rpc_frame},
        settings,
    },
    receiver::Receiver,
};
use slimevr_core::{
    pose::{PoseEngine, SceneInput},
    skeleton::BodyPosition as B,
    Vector3 as V,
};
use solarxr_protocol::rpc;

impl Service {
    pub(super) fn rpc_calibration(
        &mut self,
        h: rpc::RpcMessageHeader<'_>,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
        at: u64,
        tx: u32,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        let kind = h.message_type();
        match h.message_type() {
            rpc::RpcMessage::ClearMountingResetRequest => {
                let input = SceneInput::ClearMounting { at_ms: at };
                engine.scene_input(input.clone())?;
                self.changes.push(input);
                self.config.pose = engine.export_config();
                self.config.save(self.state_path.as_deref())?;
                out.push(settings::frame(tx, &self.config));
            }
            rpc::RpcMessage::ResetRequest => {
                let r = h.message_as_reset_request().ok_or("missing reset")?;
                reset_kind(r.reset_type())?;
                let parts = r
                    .body_parts()
                    .map(|p| p.iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                for p in &parts {
                    protocol::from_body(*p)?;
                }
                if self.resets.len() >= 8 {
                    return Err("reset queue full".into());
                }
                let seconds = r.delay().unwrap_or(match r.reset_type() {
                    rpc::ResetType::Full => self.config.pose.full_reset_delay_ms as f32 / 1000.0,
                    rpc::ResetType::Mounting => {
                        self.config.pose.mounting_reset_delay_ms as f32 / 1000.0
                    }
                    _ => 0.0,
                });
                if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
                    return Err("invalid reset delay".into());
                }
                let duration = (seconds * 1000.0).round() as u64;
                self.resets.push(PendingReset {
                    at: at.saturating_add(duration),
                    start: at,
                    kind: r.reset_type(),
                    parts: parts.clone(),
                    tx,
                    last_progress: at,
                });
                out.push(protocol::reset_frame(
                    tx,
                    r.reset_type(),
                    &parts,
                    0,
                    duration,
                    false,
                ));
            }
            rpc::RpcMessage::SetPauseTrackingRequest => {
                let r = h
                    .message_as_set_pause_tracking_request()
                    .ok_or("missing pause state")?;
                engine.set_paused(at, r.pauseTracking())?;
                self.changes.push(SceneInput::Pause {
                    at_ms: at,
                    paused: r.pauseTracking(),
                });
                self.broadcast(protocol::pause_frame(tx, r.pauseTracking()));
            }
            rpc::RpcMessage::TrackingPauseStateRequest => {
                out.push(protocol::pause_frame(tx, engine.is_paused()))
            }
            rpc::RpcMessage::EnableStayAlignedRequest => {
                let r = h
                    .message_as_enable_stay_aligned_request()
                    .ok_or("missing alignment settings")?;
                let mut c = self.config.clone();
                c.pose.alignment.enabled = r.enable();
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::LegTweaksTmpChange => {
                let r = h
                    .message_as_leg_tweaks_tmp_change()
                    .ok_or("missing temporary leg settings")?;
                let mut values = engine.leg_overrides();
                for (i, value) in [
                    r.floor_clip(),
                    r.skating_correction(),
                    r.toe_snap(),
                    r.foot_plant(),
                ]
                .into_iter()
                .enumerate()
                {
                    if let Some(v) = value {
                        values[i] = Some(v);
                    }
                }
                engine.set_leg_overrides(at, values)?;
                self.changes
                    .push(SceneInput::LegOverrides { at_ms: at, values });
            }
            rpc::RpcMessage::LegTweaksTmpClear => {
                let r = h
                    .message_as_leg_tweaks_tmp_clear()
                    .ok_or("missing temporary leg settings")?;
                let mut values = engine.leg_overrides();
                for (i, clear) in [
                    r.floor_clip(),
                    r.skating_correction(),
                    r.toe_snap(),
                    r.foot_plant(),
                ]
                .into_iter()
                .enumerate()
                {
                    if clear {
                        values[i] = None;
                    }
                }
                engine.set_leg_overrides(at, values)?;
                self.changes
                    .push(SceneInput::LegOverrides { at_ms: at, values });
            }
            rpc::RpcMessage::DetectStayAlignedRelaxedPoseRequest
            | rpc::RpcMessage::ResetStayAlignedRelaxedPoseRequest => {
                let (pose, detect) = if kind == rpc::RpcMessage::DetectStayAlignedRelaxedPoseRequest
                {
                    (
                        h.message_as_detect_stay_aligned_relaxed_pose_request()
                            .ok_or("missing relaxed pose")?
                            .pose(),
                        true,
                    )
                } else {
                    (
                        h.message_as_reset_stay_aligned_relaxed_pose_request()
                            .ok_or("missing relaxed pose")?
                            .pose(),
                        false,
                    )
                };
                let mut c = self.config.clone();
                let target = match pose {
                    rpc::StayAlignedRelaxedPose::STANDING => &mut c.pose.alignment.standing,
                    rpc::StayAlignedRelaxedPose::SITTING => &mut c.pose.alignment.sitting,
                    rpc::StayAlignedRelaxedPose::FLAT => &mut c.pose.alignment.flat,
                    _ => return Err("unknown relaxed pose".into()),
                };
                *target = if detect {
                    let qs = engine
                        .snapshot()
                        .trackers
                        .iter()
                        .filter(|p| p.status == slimevr_core::SensorStatus::Ok)
                        .filter_map(|p| p.calibrated.map(|q| (p.body, q)))
                        .collect();
                    slimevr_core::alignment::RelaxedPose::from_rotations(&qs)
                } else {
                    Default::default()
                };
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::HeightRequest => {
                // Deprecated, but still registered by the reference backend.
                let mut positions: Vec<(B, V)> = engine
                    .snapshot()
                    .trackers
                    .iter()
                    .filter(|t| t.status == slimevr_core::SensorStatus::Ok)
                    .filter_map(|t| t.position.map(|p| (t.body, p)))
                    .collect();
                positions.extend(
                    self.external
                        .iter()
                        .filter_map(|(body, pose)| pose.position.map(|p| (*body, p))),
                );
                let min_height = positions
                    .iter()
                    .map(|(_, p)| p.y)
                    .reduce(f32::min)
                    .unwrap_or(0.0);
                let max_height = positions
                    .iter()
                    .find(|(b, _)| *b == B::Head)
                    .map(|(_, p)| p.y)
                    .or_else(|| positions.iter().map(|(_, p)| p.y).reduce(f32::max))
                    .unwrap_or(0.0);
                out.push(rpc_frame(rpc::RpcMessage::HeightResponse, tx, |f| {
                    rpc::HeightResponse::create(
                        f,
                        &rpc::HeightResponseArgs {
                            min_height,
                            max_height,
                        },
                    )
                    .as_union_value()
                }));
            }
            rpc::RpcMessage::ClearDriftCompensationRequest => {
                h.message_as_clear_drift_compensation_request()
                    .ok_or("missing drift clear")?;
                // Reset-history compensation is disabled in this reference version.
                // There is consequently no active compensation history to clear.
            }
            rpc::RpcMessage::StartUserHeightCalibration => {
                engine.start_height_calibration(at)?;
                self.changes
                    .push(SceneInput::HeightCalibration { at_ms: at });
            }
            rpc::RpcMessage::CancelUserHeightCalibration => {
                engine.cancel_height_calibration(at)?;
                self.changes
                    .push(SceneInput::CancelHeightCalibration { at_ms: at });
            }
            _ => unreachable!("RPC routed to the wrong domain"),
        }
        Ok(())
    }
}
