//! Post-pose bookkeeping, diagnostics and tap setup.
use super::{recording::auto_status, Service};
use crate::{
    api::{
        diagnostics,
        protocol::{self, rpc_frame},
        settings, vrchat,
    },
    receiver::Receiver,
};
use slimevr_core::{pose::PoseEngine, skeleton::BodyPosition as B};
use solarxr_protocol as sx;
use solarxr_protocol::rpc;
use std::collections::BTreeMap;

impl Service {
    fn update_tap_setup(&mut self, receiver: &Receiver, at: u64) {
        if !self.config.pose.taps.setup_mode {
            self.setup_taps.clear();
            return;
        }
        let bindings = &self.config.pose.bindings;
        let active: Vec<_> = receiver
            .devices
            .values()
            .filter(|d| !d.transport_timed_out)
            .flat_map(|d| {
                d.sensors.iter().filter_map(move |(id, s)| {
                    let a = s.acceleration.as_ref()?;
                    (s.status == slimevr_core::SensorStatus::Ok
                        && at.saturating_sub(a.received_at_ms) <= receiver.config.sensor_timeout_ms)
                        .then_some((d.key.clone(), *id, d.session, a.value))
                })
            })
            .collect();
        let mut accelerations = BTreeMap::new();
        for (key, id, _, a) in &active {
            if let Some(b) = bindings
                .iter()
                .find(|b| b.device_key == *key && b.sensor_id == *id)
            {
                accelerations.insert(b.body, *a);
            }
        }
        let keys: std::collections::BTreeSet<_> = active
            .iter()
            .map(|(key, id, session, _)| (key.clone(), *id, *session))
            .collect();
        self.setup_taps.retain(|key, _| keys.contains(key));
        for (key, id, session, a) in active {
            let mut context = accelerations.clone();
            if let Some(b) = bindings
                .iter()
                .find(|b| b.device_key == key && b.sensor_id == id)
            {
                context.remove(&b.body);
            }
            context.insert(B::Head, a);
            if self
                .setup_taps
                .entry((key.clone(), id, session))
                .or_default()
                .update(at, B::Head, &context, 2, self.config.pose.taps.max_moving)
            {
                if let Some(device_id) = self.config.device_ids.get(&key) {
                    self.broadcast(rpc_frame(
                        rpc::RpcMessage::TapDetectionSetupNotification,
                        0,
                        |f| {
                            let device = sx::datatypes::DeviceId::new(*device_id);
                            let tracker = sx::datatypes::TrackerId::create(
                                f,
                                &sx::datatypes::TrackerIdArgs {
                                    device_id: Some(&device),
                                    tracker_num: id,
                                },
                            );
                            rpc::TapDetectionSetupNotification::create(
                                f,
                                &rpc::TapDetectionSetupNotificationArgs {
                                    tracker_id: Some(tracker),
                                },
                            )
                            .as_union_value()
                        },
                    ));
                }
            }
        }
    }
    pub fn after_tick(&mut self, engine: &PoseEngine, receiver: &Receiver, at: u64) {
        self.sync_pose_config(engine);
        self.update_tap_setup(receiver, at);
        if at >= self.next_diagnostics {
            self.next_diagnostics = at.saturating_add(1000);
            self.diagnostics.rest = receiver
                .devices
                .values()
                .filter(|d| {
                    !d.transport_timed_out
                        && matches!(
                            d.origin,
                            crate::receiver::Origin::Udp | crate::receiver::Origin::Hid
                        )
                })
                .flat_map(|d| {
                    d.sensors
                        .iter()
                        .map(|(id, s)| ((d.key.clone(), *id), s.info.rest_calibrated))
                })
                .collect();
            self.diagnostics.vrchat = self
                .vrchat
                .as_ref()
                .map(|v| vrchat::healthy(v, &self.config, engine.snapshot()));
            let frame = vrchat::frame(0, self.vrchat.as_ref(), &self.config, engine.snapshot());
            if frame != self.last_vrchat {
                self.last_vrchat = frame.clone();
                self.broadcast(frame);
            }
            self.diagnostics.controllers = [B::LeftHand, B::RightHand]
                .iter()
                .any(|b| self.external.get(b).is_some_and(|p| p.position.is_some()));
            let checklist = diagnostics::frame(
                0,
                engine.snapshot(),
                &self.config,
                &self.steam_vr,
                &self.driver_status,
                &self.diagnostics,
            );
            if checklist != self.last_checklist {
                self.last_checklist = checklist.clone();
                self.broadcast(checklist);
            }
            for frame in self.statuses.refresh(
                engine.snapshot(),
                &self.config,
                &self.steam_vr,
                &self.diagnostics,
            ) {
                self.broadcast(frame);
            }
        }
        let snapshot = engine.snapshot();
        if let Some(recorder) = &mut self.bvh {
            if let Err(e) = recorder.tick(&snapshot.skeleton, snapshot.tick_dt_seconds) {
                self.error(e);
                self.finish_bvh();
            }
        }
        let mut ids_changed = false;
        for key in receiver.devices.keys() {
            if !self.config.device_ids.contains_key(key) {
                if let Some(id) =
                    (1..=254).find(|id| !self.config.device_ids.values().any(|v| v == id))
                {
                    self.config.device_ids.insert(key.clone(), id);
                    ids_changed = true;
                }
            }
        }
        let p = engine.snapshot();
        if p.reset_count != self.last_reset && (p.mounting_completed || p.feet_mounting_completed) {
            if let Err(e) = crate::config::put(
                &mut self.config.yaml,
                &["resetsConfig", "lastMountingMethod"],
                "AUTOMATIC",
            ) {
                self.error(e.to_string());
            }
        }
        if ids_changed || p.reset_count != self.last_reset || p.height_status != self.last_height {
            if p.height_status != self.last_height {
                self.broadcast(protocol::height_frame(p));
            }
            self.last_reset = p.reset_count;
            self.last_height = p.height_status;
            if let Err(e) = self.persist_config() {
                self.error(e);
            }
            self.broadcast(settings::frame(0, &self.config));
            self.broadcast(diagnostics::frame(
                0,
                p,
                &self.config,
                &self.steam_vr,
                &self.driver_status,
                &self.diagnostics,
            ));
        }
        if self.recording && at >= self.next_sample {
            self.next_sample = at.saturating_add(self.config.sample_ms);
            match engine.motion_frame() {
                Ok(frame) => {
                    if !p.paused && self.frames.last().is_none_or(|f| f.at_ms < frame.at_ms) {
                        self.frames.push(frame);
                    }
                    let done = self.frames.len() >= self.config.sample_count;
                    self.broadcast(auto_status(
                        rpc::AutoBoneProcessType::RECORD,
                        self.frames.len(),
                        self.config.sample_count,
                        false,
                        false,
                    ));
                    if done {
                        self.recording = false;
                        self.save_recording(true);
                    }
                }
                Err(e) => {
                    self.recording = false;
                    self.broadcast(auto_status(
                        rpc::AutoBoneProcessType::RECORD,
                        0,
                        self.config.sample_count,
                        true,
                        false,
                    ));
                    self.error(e);
                }
            }
        }
    }
}
