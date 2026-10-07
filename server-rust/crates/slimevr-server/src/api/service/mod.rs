//! Single-owner API state and shared configuration commit / observation restore.
mod calibration;
mod devices;
mod legacy;
mod notifications;
mod recording;
mod rpc;
mod sources;
mod tick;
use self::calibration::PendingReset;
use crate::{
    api::{
        device_control, diagnostics, protocol, settings, status, types::error_wire, vrchat,
        FrontendConfig, LiveState, Request, Wire,
    },
    receiver::Receiver,
};
use serde_json::json;
use slimevr_core::{
    autobone::{AutoBoneResult, MotionFrame},
    pose::{PoseEngine, SceneInput, TrackerBinding},
    skeleton::{BodyPosition as B, HeadPose},
    EventKind, InputEvent, TrackerSample,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{broadcast, mpsc};

pub struct Service {
    pub config: FrontendConfig,
    pub state_path: Option<PathBuf>,
    pub commands: mpsc::Sender<Request>,
    pub events: broadcast::Sender<Wire>,
    pub changes: Vec<SceneInput>,
    pub forgotten_devices: Vec<String>,
    pub external: BTreeMap<B, HeadPose>,
    pub steam_vr: crate::steamvr::Status,
    pub device_control: device_control::Controller,
    pub driver_manager: Option<crate::steamvr::manager::Manager>,
    pub driver_status: crate::steamvr::manager::DriverStatus,
    pub local_ip: std::net::IpAddr,
    pub serial: Option<crate::serial::Controller>,
    pub hid: Option<crate::hid::Controller>,
    pub provisioning: crate::serial::provisioning::Provisioner,
    pub firmware: crate::firmware::Controller,
    pub diagnostics: diagnostics::Context,
    last_checklist: Vec<u8>,
    next_diagnostics: u64,
    statuses: status::Store,
    pub vrchat: Option<vrchat::Values>,
    last_vrchat: Vec<u8>,
    resets: Vec<PendingReset>,
    key_pauses: Vec<u64>,
    steam_sources: BTreeSet<B>,
    frames: Vec<MotionFrame>,
    recording: bool,
    saving: bool,
    save_after_record: bool,
    setup_taps: BTreeMap<(String, u8, u64), slimevr_core::gestures::TapDetector>,
    bvh: Option<crate::bvh::Recorder>,
    next_sample: u64,
    processing: bool,
    result: Option<AutoBoneResult>,
    last_reset: u64,
    last_height: slimevr_core::gestures::HeightStatus,
}
impl Service {
    pub fn new(
        config: FrontendConfig,
        path: Option<PathBuf>,
        commands: mpsc::Sender<Request>,
        events: broadcast::Sender<Wire>,
    ) -> Self {
        Self {
            config,
            state_path: path,
            commands,
            events,
            changes: Vec::new(),
            forgotten_devices: Vec::new(),
            external: BTreeMap::new(),
            steam_vr: Default::default(),
            steam_sources: BTreeSet::new(),
            device_control: device_control::Controller::default(),
            driver_manager: None,
            driver_status: Default::default(),
            local_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            serial: None,
            hid: None,
            provisioning: Default::default(),
            firmware: Default::default(),
            diagnostics: diagnostics::Context::detect(),
            last_checklist: Vec::new(),
            next_diagnostics: 0,
            statuses: status::Store::default(),
            vrchat: None,
            last_vrchat: vec![],
            resets: Vec::new(),
            key_pauses: Vec::new(),
            frames: Vec::new(),
            recording: false,
            saving: false,
            save_after_record: false,
            setup_taps: BTreeMap::new(),
            bvh: None,
            next_sample: 0,
            processing: false,
            result: None,
            last_reset: 0,
            last_height: slimevr_core::gestures::HeightStatus::Idle,
        }
    }
    fn broadcast(&self, b: Vec<u8>) {
        let _ = self.events.send(Wire::Binary(b));
    }
    pub fn error(&self, error: impl std::fmt::Display) {
        let message = error.to_string();
        crate::logging::diagnostic(
            crate::log_level::LogLevel::Error,
            &json!({"type":"backend_error", "message":message}),
        );
        let _ = self.events.send(error_wire(message));
    }
    fn commit(
        &mut self,
        config: FrontendConfig,
        engine: &mut PoseEngine,
        receiver: &Receiver,
        at: u64,
    ) -> Result<(), String> {
        let old_bindings = self.config.pose.bindings.clone();
        if self.config.pose.taps.setup_mode != config.pose.taps.setup_mode
            || self.config.pose.taps.max_moving != config.pose.taps.max_moving
        {
            self.setup_taps.clear();
        }
        config.validate()?;
        config.save(self.state_path.as_deref())?;
        engine.configure(at, config.pose.clone())?;
        self.changes.push(SceneInput::Configure {
            at_ms: at,
            config: Box::new(config.pose.clone()),
        });
        self.config = config;
        if let Some(hid) = &self.hid {
            hid.set_direct(
                self.config.yaml["hidConfig"]["trackersOverHID"]
                    .as_bool()
                    .unwrap_or(false),
            );
        }
        self.seed(engine, receiver, at, &old_bindings)?;
        self.broadcast(settings::frame(0, &self.config));
        self.broadcast(protocol::skeleton_frame(0, &self.config.pose));
        Ok(())
    }
    fn seed(
        &mut self,
        engine: &mut PoseEngine,
        receiver: &Receiver,
        at: u64,
        previous: &[TrackerBinding],
    ) -> Result<(), String> {
        for b in self.config.pose.bindings.clone() {
            if previous.contains(&b) {
                continue;
            }
            if let Some(d) = receiver.devices.get(&b.device_key) {
                if let Some(s) = d.sensors.get(&b.sensor_id) {
                    let sample = TrackerSample {
                        source: if d.origin == crate::receiver::Origin::Hid {
                            "hid"
                        } else {
                            "udp"
                        }
                        .into(),
                        device_key: b.device_key.clone(),
                        sensor_id: b.sensor_id,
                        session: d.session,
                        packet_sequence: d.last_sequence,
                        received_at_ms: s.rotation.as_ref().map_or(at, |r| r.received_at_ms),
                        sensor_timestamp_us: None,
                        packet_rotation: None,
                        server_rotation: s.rotation.as_ref().map(|r| r.value),
                        packet_acceleration: None,
                        server_acceleration: None,
                        position: s.position.as_ref().map(|v| v.value),
                        compatibility_fallback: false,
                    };
                    engine.restore_sample(at, &sample)?;
                    self.changes
                        .push(SceneInput::RestoreSample { at_ms: at, sample });
                    let event = InputEvent {
                        at_ms: at,
                        kind: EventKind::SensorMetadata {
                            device_key: b.device_key.clone(),
                            sensor_id: b.sensor_id,
                            imu_type: s.info.imu_type,
                            data_type: s.info.data_type,
                            magnetometer_enabled: s.info.config.is_some_and(|v| v & 3 == 3),
                        },
                    };
                    engine.ingest(&event)?;
                    self.changes.push(SceneInput::Input { event });
                    let event = InputEvent {
                        at_ms: at,
                        kind: EventKind::SensorState {
                            device_key: b.device_key,
                            sensor_id: b.sensor_id,
                            status: s.status,
                        },
                    };
                    engine.ingest(&event)?;
                    self.changes.push(SceneInput::Input { event });
                }
            }
        }
        Ok(())
    }
    pub fn live(&self, receiver: &Receiver, engine: &PoseEngine, at: u64) -> Arc<LiveState> {
        Arc::new(LiveState {
            at,
            devices: receiver.devices.clone(),
            config: self.config.clone(),
            pose: engine.snapshot().clone(),
            external: self.external.clone(),
            persistent: self.state_path.is_some(),
            steam_vr: self.steam_vr.clone(),
            driver_status: self.driver_status.clone(),
        })
    }
}
