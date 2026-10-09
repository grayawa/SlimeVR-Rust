//! SteamVR driver protocol v2. IPC transports messages; the runtime owns pose updates.
pub mod manager;
mod output_stats;
pub mod transport;
pub use output_stats::{OutputCounters, OutputStats};
pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}
use crate::{api::FrontendConfig, receiver::Receiver};
use messages::{protobuf_message::Message as M, ProtobufMessage};
use serde::{Deserialize, Serialize};
use slimevr_core::{
    calibration::ResetKind,
    pose::{PoseSnapshot, SceneInput},
    skeleton::BodyPosition as B,
    Quaternion as Q, Vector3 as V,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{atomic::Ordering, Arc},
};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub trackers: BTreeMap<String, bool>,
    #[serde(rename = "automaticSharedTrackersToggling")]
    pub automatic: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            trackers: ROLES
                .iter()
                .map(|(_, role, _)| ((*role).into(), false))
                .collect(),
            automatic: true,
        }
    }
}
impl Settings {
    pub fn enabled(&self, role: &str) -> bool {
        self.trackers.get(role).copied().unwrap_or(false)
    }
    pub fn update_automatic(&mut self, snapshot: &PoseSnapshot) -> bool {
        if !self.automatic || snapshot.paused {
            return false;
        }
        let assigned: BTreeSet<_> = snapshot
            .trackers
            .iter()
            .filter(|p| !p.device_key.starts_with("steamvr:"))
            .map(|p| p.body)
            .collect();
        let mut changed = false;
        for (role, bodies) in [
            ("waist", &[B::UpperChest, B::Chest, B::Waist, B::Hip][..]),
            ("chest", &[B::UpperChest, B::Chest][..]),
            ("left_knee", &[B::LeftUpperLeg][..]),
            ("right_knee", &[B::RightUpperLeg][..]),
            ("left_foot", &[B::LeftLowerLeg, B::LeftFoot][..]),
            ("right_foot", &[B::RightLowerLeg, B::RightFoot][..]),
            ("left_elbow", &[B::LeftUpperArm][..]),
            ("right_elbow", &[B::RightUpperArm][..]),
        ] {
            let enabled = if role == "waist" {
                snapshot
                    .trackers
                    .iter()
                    .any(|p| matches!(p.body, B::UpperChest | B::Chest | B::Waist | B::Hip))
                    && !snapshot.trackers.iter().any(|p| {
                        matches!(p.body, B::Waist | B::Hip) && p.device_key.starts_with("steamvr:")
                    })
            } else {
                bodies.iter().any(|b| assigned.contains(b))
            };
            if self.enabled(role) != enabled {
                self.trackers.insert(role.into(), enabled);
                changed = true;
            }
        }
        changed
    }
}

// Stable local IDs and original serials survive driver reconnections. Incoming IDs
// belong to the driver and may overlap: they are in an independent namespace.
pub const ROLES: [(i32, &str, &str); 11] = [
    (15, "head", "head"),
    (4, "chest", "chest"),
    (1, "waist", "hip"),
    (5, "left_knee", "left_knee"),
    (6, "right_knee", "right_knee"),
    (2, "left_foot", "left_foot"),
    (3, "right_foot", "right_foot"),
    (7, "left_elbow", "left_elbow"),
    (8, "right_elbow", "right_elbow"),
    (11, "left_hand", "left_hand"),
    (12, "right_hand", "right_hand"),
];
pub fn envelope(message: M) -> ProtobufMessage {
    ProtobufMessage {
        message: Some(message),
    }
}

#[derive(Debug)]
pub enum Event {
    Feeder(Box<Event>),
    Connected(u64),
    Disconnected(u64),
    Message(u64, ProtobufMessage),
}
pub struct Batch {
    pub session: u64,
    pub messages: Vec<ProtobufMessage>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub available: bool,
    pub connected: bool,
    pub protocol_version: Option<i32>,
    pub endpoint: Option<PathBuf>,
    pub bindings_provider_running: bool,
    pub last_error: Option<String>,
}
struct Remote {
    key: Option<String>,
    body: Option<B>,
    status: i32,
    last_update: u64,
    rotation: Option<Q>,
    position: Option<V>,
}
#[derive(Default)]
pub struct Session {
    pub status: Status,
    session: u64,
    remote: BTreeMap<i32, Remote>,
    // Only the OpenVR device that last supplied this pose may retire it. Hand
    // tracking and controllers can coexist under different IDs for one body.
    published: BTreeMap<B, i32>,
    shared: BTreeSet<String>,
    ready: bool,
    initialized: bool,
    output_stats: Arc<OutputStats>,
}
impl Session {
    pub fn output_stats(&self) -> &Arc<OutputStats> {
        &self.output_stats
    }
    pub fn current(&self, event: &Event) -> bool {
        matches!(event, Event::Message(session,_) if *session==self.session && self.status.connected)
    }
    pub fn disconnect(&mut self, at: u64) -> Vec<SceneInput> {
        self.status.connected = false;
        self.status.protocol_version = None;
        self.clear(at)
    }
    pub fn receive(&mut self, event: Event, at: u64) -> Result<Vec<SceneInput>, String> {
        let mut inputs = Vec::new();
        match event {
            Event::Connected(session) => {
                inputs = self.clear(at);
                self.session = session;
                self.status.connected = true;
                self.status.protocol_version = None;
                self.status.last_error = None;
                self.shared.clear();
                self.initialized = false;
                self.ready = false;
            }
            Event::Disconnected(session) if session == self.session => {
                inputs = self.clear(at);
                self.status.connected = false;
                self.status.protocol_version = None;
                self.shared.clear();
            }
            Event::Message(session, message)
                if session == self.session && self.status.connected =>
            {
                self.ready = true;
                match message.message {
                    Some(M::Version(v)) => {
                        self.status.protocol_version = Some(v.protocol_version);
                        if !matches!(v.protocol_version, 1 | 2) {
                            self.status.last_error = Some(format!(
                                "Driver protocol {} differs from server protocol 2",
                                v.protocol_version
                            ));
                        }
                    }
                    Some(M::TrackerAdded(t)) => {
                        if !(0..64).contains(&t.tracker_id)
                            || t.tracker_serial.len() > 256
                            || t.tracker_name.len() > 256
                        {
                            return Err("invalid SteamVR tracker metadata".into());
                        }
                        let body = if t.tracker_serial.starts_with("human://") {
                            None
                        } else if t.tracker_id == 0 || matches!(t.tracker_role, 15 | 19) {
                            Some(B::Head)
                        } else {
                            match t.tracker_role {
                                11 | 13 => Some(B::LeftHand),
                                12 | 14 => Some(B::RightHand),
                                _ => None,
                            }
                        };
                        let key = if body.is_none() && !t.tracker_serial.starts_with("human://") {
                            Some(source_key("steamvr", &t.tracker_serial))
                        } else {
                            None
                        };
                        if let Some(key) = &key {
                            inputs.push(external(
                                at,
                                slimevr_core::EventKind::ExternalTracker {
                                    device_key: key.clone(),
                                    sensor_id: 0,
                                    source: "steamvr".into(),
                                    name: t.tracker_serial.clone(),
                                    body: role_body(t.tracker_role),
                                    capabilities: slimevr_core::TrackerCapabilities {
                                        allow_reset: true,
                                        allow_mounting: false,
                                        allow_filter: false,
                                        is_imu: false,
                                    },
                                },
                            ));
                            inputs.push(external(
                                at,
                                slimevr_core::EventKind::DeviceConnected {
                                    device_key: key.clone(),
                                    address: "steamvr".into(),
                                    firmware: None,
                                    session: self.session,
                                    preserve_calibration: false,
                                },
                            ));
                        }
                        if let Some(previous) = self.remote.insert(
                            t.tracker_id,
                            Remote {
                                key,
                                body,
                                status: 1,
                                last_update: at,
                                rotation: None,
                                position: None,
                            },
                        ) {
                            if let Some(key) = previous.key {
                                inputs.push(external(
                                    at,
                                    slimevr_core::EventKind::SensorState {
                                        device_key: key,
                                        sensor_id: 0,
                                        status: slimevr_core::SensorStatus::Disconnected,
                                    },
                                ));
                            }
                            if let Some(body) = previous
                                .body
                                .filter(|body| self.published.get(body) == Some(&t.tracker_id))
                            {
                                inputs.push(SceneInput::ClearSource { at_ms: at, body });
                                self.published.remove(&body);
                            }
                        }
                    }
                    Some(M::TrackerStatus(s)) => {
                        if let Some(remote) = self.remote.get_mut(&s.tracker_id) {
                            remote.status = s.status;
                            if let Some(key) = &remote.key {
                                inputs.push(external(
                                    at,
                                    slimevr_core::EventKind::SensorState {
                                        device_key: key.clone(),
                                        sensor_id: 0,
                                        status: match s.status {
                                            1 => slimevr_core::SensorStatus::Ok,
                                            2 => slimevr_core::SensorStatus::Busy,
                                            4 => slimevr_core::SensorStatus::Occluded,
                                            3 => slimevr_core::SensorStatus::Error,
                                            _ => slimevr_core::SensorStatus::Disconnected,
                                        },
                                    },
                                ));
                            }
                            if !matches!(s.status, 1 | 2 | 4) {
                                if let Some(body) = remote.body {
                                    if self.published.get(&body) == Some(&s.tracker_id) {
                                        inputs.push(SceneInput::ClearSource { at_ms: at, body });
                                        self.published.remove(&body);
                                    }
                                    remote.rotation = None;
                                    remote.position = None;
                                }
                            }
                        }
                    }
                    Some(M::Position(p)) => {
                        let Some(remote) = self.remote.get_mut(&p.tracker_id) else {
                            return Ok(inputs);
                        };
                        if remote.body.is_none() && remote.key.is_none() {
                            return Ok(inputs);
                        }
                        if !matches!(remote.status, 1 | 2 | 4) {
                            return Ok(inputs);
                        }
                        let rotation = Q::new(p.qw, p.qx, p.qy, p.qz);
                        if !rotation.is_rotation() {
                            return Err("invalid SteamVR rotation".into());
                        }
                        let position = match (p.x, p.y, p.z) {
                            (Some(x), Some(y), Some(z)) => {
                                let v = V::new(x, y, z);
                                if !v.is_finite() {
                                    return Err("invalid SteamVR position".into());
                                }
                                Some(v)
                            }
                            (None, None, None) => remote.position,
                            _ => return Err("incomplete SteamVR position".into()),
                        };
                        if [p.vx, p.vy, p.vz]
                            .into_iter()
                            .flatten()
                            .any(|v| !v.is_finite())
                        {
                            return Err("invalid SteamVR velocity".into());
                        }
                        // Rotation-only tracking can orient the head, but cannot
                        // provide a live controller/world-position anchor.
                        let position = if remote.status == 4 || p.data_source == Some(1) {
                            None
                        } else {
                            position
                        };
                        remote.rotation = Some(rotation);
                        remote.position = position;
                        remote.last_update = at;
                        if let Some(key) = &remote.key {
                            inputs.push(external(
                                at,
                                slimevr_core::EventKind::SensorState {
                                    device_key: key.clone(),
                                    sensor_id: 0,
                                    status: if remote.status == 4 {
                                        slimevr_core::SensorStatus::Occluded
                                    } else if remote.status == 2 {
                                        slimevr_core::SensorStatus::Busy
                                    } else {
                                        slimevr_core::SensorStatus::Ok
                                    },
                                },
                            ));
                            inputs.push(external(
                                at,
                                slimevr_core::EventKind::Sample {
                                    sample: slimevr_core::TrackerSample {
                                        source: "steamvr".into(),
                                        device_key: key.clone(),
                                        sensor_id: 0,
                                        session: self.session,
                                        packet_sequence: at as i64,
                                        received_at_ms: at,
                                        socket_received_at_ms: None,
                                        sensor_timestamp_us: None,
                                        packet_rotation: Some(rotation),
                                        server_rotation: Some(rotation),
                                        packet_acceleration: None,
                                        server_acceleration: None,
                                        position,
                                        compatibility_fallback: false,
                                    },
                                },
                            ));
                            return Ok(inputs);
                        }
                        let Some(body) = remote.body else {
                            return Ok(inputs);
                        };
                        match body {
                            B::Head => inputs.push(SceneInput::Head {
                                at_ms: at,
                                rotation,
                                position,
                            }),
                            _ => {
                                if let Some(position) = position {
                                    inputs.push(SceneInput::Controller {
                                        at_ms: at,
                                        body,
                                        rotation,
                                        position,
                                    });
                                } else {
                                    if self.published.get(&body) == Some(&p.tracker_id) {
                                        inputs.push(SceneInput::ClearSource { at_ms: at, body });
                                        self.published.remove(&body);
                                    }
                                    return Ok(inputs);
                                }
                            }
                        }
                        self.published.insert(body, p.tracker_id);
                    }
                    Some(M::UserAction(action)) => match action.name.as_str() {
                        "reset" => inputs.push(SceneInput::Reset {
                            at_ms: at,
                            kind: ResetKind::Full,
                        }),
                        "fast_reset" => inputs.push(SceneInput::Reset {
                            at_ms: at,
                            kind: ResetKind::Yaw,
                        }),
                        "mounting_reset" => inputs.push(SceneInput::Reset {
                            at_ms: at,
                            kind: ResetKind::Mounting,
                        }),
                        "pause_tracking" => {} // Runtime supplies the current paused state.
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(inputs)
    }
    fn clear(&mut self, at: u64) -> Vec<SceneInput> {
        let mut inputs: Vec<_> = self
            .remote
            .values()
            .filter_map(|r| r.body)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|body| SceneInput::ClearSource { at_ms: at, body })
            .collect();
        inputs.extend(
            self.remote
                .values()
                .filter_map(|r| r.key.as_ref())
                .map(|key| {
                    external(
                        at,
                        slimevr_core::EventKind::SensorState {
                            device_key: key.clone(),
                            sensor_id: 0,
                            status: slimevr_core::SensorStatus::Disconnected,
                        },
                    )
                }),
        );
        self.remote.clear();
        self.published.clear();
        inputs
    }
    pub fn expire(&mut self, at: u64) -> Vec<SceneInput> {
        let mut inputs = Vec::new();
        for (id, remote) in &mut self.remote {
            if remote.key.is_some()
                && remote.rotation.is_some()
                && at.saturating_sub(remote.last_update) > 500
            {
                inputs.push(external(
                    at,
                    slimevr_core::EventKind::SensorState {
                        device_key: remote.key.clone().unwrap(),
                        sensor_id: 0,
                        status: slimevr_core::SensorStatus::TimedOut,
                    },
                ));
                remote.rotation = None;
                remote.position = None;
            }
            if let Some(body) = remote.body {
                if at.saturating_sub(remote.last_update) > 500 {
                    if self.published.get(&body) == Some(id) {
                        self.published.remove(&body);
                        inputs.push(SceneInput::ClearSource { at_ms: at, body });
                    }
                    remote.rotation = None;
                    remote.position = None;
                }
            }
        }
        inputs
    }
    pub fn output(
        &mut self,
        config: &FrontendConfig,
        pose: &PoseSnapshot,
        receiver: &Receiver,
        sender: &mpsc::Sender<Batch>,
    ) {
        if !self.status.connected {
            return;
        }
        let shared: BTreeSet<_> = ROLES
            .iter()
            .filter(|(_, role, _)| config.steam_vr.enabled(role))
            .map(|(_, role, _)| (*role).to_owned())
            .collect();
        let mut messages = Vec::new();
        if !self.initialized {
            messages.push(envelope(M::Version(messages::Version {
                protocol_version: 2,
            })));
        }
        for (role_id, role, name) in ROLES {
            let id = role_id; // Stable local namespace, independent of driver input IDs.
            if shared.contains(role) && !self.shared.contains(role) {
                let serial = format!("human://{}", role.to_ascii_uppercase());
                messages.push(envelope(M::TrackerAdded(messages::TrackerAdded {
                    tracker_id: id,
                    tracker_serial: serial.clone(),
                    tracker_name: serial,
                    tracker_role: role_id,
                    manufacturer: "SlimeVR".into(),
                })));
            } else if !shared.contains(role) && self.shared.contains(role) {
                messages.push(envelope(M::TrackerStatus(messages::TrackerStatus {
                    tracker_id: id,
                    status: 0,
                    ..Default::default()
                })));
            }
            if !shared.contains(role) || !self.ready {
                continue;
            }
            let Some(p) = pose.skeleton.computed.get(name) else {
                continue;
            };
            let valid = pose.skeleton.world_anchor_present;
            messages.push(envelope(M::TrackerStatus(messages::TrackerStatus {
                tracker_id: id,
                status: if valid { 1 } else { 3 },
                ..Default::default()
            })));
            if valid {
                let velocity = config
                    .pose
                    .send_derived_velocity
                    .then(|| pose.computed_velocities.get(name))
                    .flatten();
                messages.push(envelope(M::Position(messages::Position {
                    tracker_id: id,
                    x: Some(p.position.x),
                    y: Some(p.position.y),
                    z: Some(p.position.z),
                    qw: p.rotation.w,
                    qx: p.rotation.x,
                    qy: p.rotation.y,
                    qz: p.rotation.z,
                    vx: velocity.map(|v| v.x),
                    vy: velocity.map(|v| v.y),
                    vz: velocity.map(|v| v.z),
                    ..Default::default()
                })));
                if let Some((level, charging)) = battery(role, config, receiver) {
                    messages.push(envelope(M::Battery(messages::Battery {
                        tracker_id: id,
                        battery_level: level,
                        is_charging: charging,
                    })));
                }
            }
        }
        if !messages.is_empty() {
            match sender.try_send(Batch {
                session: self.session,
                messages,
            }) {
                Ok(()) => {
                    self.output_stats.enqueued.fetch_add(1, Ordering::Relaxed);
                    self.shared = shared;
                    self.initialized = true;
                    self.ready = false;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.output_stats.queue_full.fetch_add(1, Ordering::Relaxed);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.output_stats
                        .queue_closed
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}
fn battery(role: &str, config: &FrontendConfig, receiver: &Receiver) -> Option<(f32, bool)> {
    let bodies: &[B] = match role {
        "waist" => {
            if config.steam_vr.enabled("chest") {
                &[B::Waist, B::Hip]
            } else {
                &[B::Waist, B::Hip, B::UpperChest, B::Chest]
            }
        }
        "chest" => {
            if config.steam_vr.enabled("waist") {
                &[B::UpperChest, B::Chest]
            } else {
                &[B::UpperChest, B::Chest, B::Waist, B::Hip]
            }
        }
        "left_foot" => {
            if config.steam_vr.enabled("left_knee") {
                &[B::LeftLowerLeg, B::LeftFoot]
            } else {
                &[B::LeftLowerLeg, B::LeftFoot, B::LeftUpperLeg]
            }
        }
        "right_foot" => {
            if config.steam_vr.enabled("right_knee") {
                &[B::RightLowerLeg, B::RightFoot]
            } else {
                &[B::RightLowerLeg, B::RightFoot, B::RightUpperLeg]
            }
        }
        "left_knee" => &[B::LeftUpperLeg],
        "right_knee" => &[B::RightUpperLeg],
        "left_elbow" => &[B::LeftUpperArm, B::LeftLowerArm],
        "right_elbow" => &[B::RightUpperArm, B::RightLowerArm],
        "left_hand" => &[B::LeftHand],
        "right_hand" => &[B::RightHand],
        "head" => &[B::Head],
        _ => &[],
    };
    config
        .pose
        .bindings
        .iter()
        .filter(|b| bodies.contains(&b.body))
        .filter_map(|b| receiver.devices.get(&b.device_key))
        .filter_map(|d| {
            let level = d.battery_fraction.as_ref()?.value * 100.0;
            let voltage = d.battery_voltage.as_ref().map_or(0.0, |v| v.value);
            (level.is_finite()
                && (0.0..=100.0).contains(&level)
                && !(voltage < 3.2 && level <= 0.0))
                .then_some((level / 100.0, voltage >= 4.3))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

pub fn default_endpoint() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"\\.\pipe\SlimeVRDriver")
    }
    #[cfg(not(windows))]
    {
        let pressure = std::env::var_os("PRESSURE_VESSEL_RUNTIME").is_some_and(|v| !v.is_empty());
        let directory = std::env::var_os("SLIMEVR_SOCKET_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                if pressure {
                    std::env::var_os("XDG_DATA_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
                        })
                        .map(|p| p.join("dev.slimevr.SlimeVR"))
                } else {
                    None
                }
            })
            .or_else(|| std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from))
            .unwrap_or_else(std::env::temp_dir);
        directory.join("SlimeVRDriver")
    }
}

pub struct Bridge {
    pub state: Session,
    pub events: mpsc::Receiver<Event>,
    pub output: mpsc::Sender<Batch>,
    task: JoinHandle<()>,
    rpc_task: JoinHandle<()>,
    feeder_task: JoinHandle<()>,
    feeder_forward: JoinHandle<()>,
    feeder: Session,
    feeder_output: mpsc::Sender<Batch>,
    feeder_disabled: bool,
    provider_path: Option<PathBuf>,
    provider: Option<tokio::process::Child>,
    provider_due: Option<u64>,
}
impl Bridge {
    pub fn receive(&mut self, event: Event, at: u64) -> Result<Vec<SceneInput>, String> {
        if let Event::Feeder(event) = event {
            if self.feeder_disabled {
                return Ok(Vec::new());
            }
            let event = match *event {
                Event::Connected(s) => Event::Connected(s | (1 << 63)),
                Event::Disconnected(s) => Event::Disconnected(s | (1 << 63)),
                Event::Message(s, m) => Event::Message(s | (1 << 63), m),
                _ => return Ok(Vec::new()),
            };
            let inputs = self.feeder.receive(event, at)?;
            if self.feeder.status.connected {
                let _ = self.feeder_output.try_send(Batch {
                    session: self.feeder.session & !(1 << 63),
                    messages: vec![envelope(M::Version(messages::Version {
                        protocol_version: 2,
                    }))],
                });
            }
            return Ok(inputs);
        }
        if matches!(&event, Event::Connected(_) | Event::Disconnected(_)) {
            self.provider = None;
            self.provider_due = None;
            self.state.status.bindings_provider_running = false;
        }
        let mut inputs = self.state.receive(event, at)?;
        if !self.feeder_disabled && self.state.status.protocol_version.is_some_and(|v| v >= 2) {
            self.feeder_disabled = true;
            self.feeder_task.abort();
            self.feeder_forward.abort();
            inputs.extend(self.feeder.disconnect(at));
        }
        Ok(inputs)
    }
    pub fn expire(&mut self, at: u64) -> Vec<SceneInput> {
        let mut inputs = self.state.expire(at);
        if !self.feeder_disabled {
            inputs.extend(self.feeder.expire(at));
        }
        inputs
    }
    pub fn start(
        endpoint: &Path,
        provider: Option<PathBuf>,
        no_provider: bool,
        commands: mpsc::Sender<crate::api::Request>,
        publisher: tokio::sync::broadcast::Sender<crate::api::Wire>,
        hub: crate::api::pubsub::Hub,
    ) -> io::Result<Self> {
        let listener = transport::Listener::bind(endpoint)?;
        let rpc_endpoint = if cfg!(windows) {
            PathBuf::from(r"\\.\pipe\SlimeVRRpc")
        } else {
            endpoint.with_file_name("SlimeVRRpc")
        };
        let rpc_listener = transport::Listener::bind(&rpc_endpoint)?;
        let (sender, events) = mpsc::channel(128);
        let (output, receiver) = mpsc::channel(4);
        let feeder_endpoint = if cfg!(windows) {
            PathBuf::from(r"\\.\pipe\SlimeVRInput")
        } else {
            endpoint.with_file_name("SlimeVRInput")
        };
        let feeder_listener = transport::Listener::bind(&feeder_endpoint)?;
        let (feeder_events, mut incoming) = mpsc::channel(128);
        let (feeder_output, feeder_receive) = mpsc::channel(4);
        let feeder_task = tokio::spawn(feeder_listener.run(feeder_events, feeder_receive));
        let forward = sender.clone();
        let feeder_forward = tokio::spawn(async move {
            while let Some(event) = incoming.recv().await {
                if forward.send(Event::Feeder(Box::new(event))).await.is_err() {
                    break;
                }
            }
        });
        let output_stats = Arc::new(OutputStats::default());
        let task = tokio::spawn(listener.run_with_stats(sender, receiver, output_stats.clone()));
        let rpc_task = tokio::spawn(rpc_listener.run_rpc(commands, publisher, hub));
        let provider_path = if no_provider {
            None
        } else {
            provider.or_else(find_provider)
        };
        Ok(Self {
            state: Session {
                output_stats,
                status: Status {
                    available: true,
                    endpoint: Some(endpoint.into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            events,
            output,
            task,
            rpc_task,
            feeder_task,
            feeder_forward,
            feeder: Session::default(),
            feeder_output,
            feeder_disabled: false,
            provider_path,
            provider: None,
            provider_due: None,
        })
    }
    pub fn provider_tick(&mut self, at: u64) {
        if !self.state.status.connected {
            self.provider_due = None;
            self.provider = None;
        } else if self.state.status.protocol_version.is_some_and(|v| v >= 2)
            && self.provider.is_none()
            && self.provider_due.is_none()
        {
            self.provider_due = Some(at.saturating_add(3000));
        }
        if self.provider_due.is_some_and(|due| at >= due) {
            self.provider_due = Some(u64::MAX); // One launch attempt per connection.
            if let Some(path) = &self.provider_path {
                let mut command = tokio::process::Command::new(path);
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    command.current_dir(parent);
                    #[cfg(target_os = "linux")]
                    {
                        let mut directories = vec![parent.to_path_buf()];
                        if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
                            directories.extend(std::env::split_paths(&existing));
                        }
                        if let Ok(paths) = std::env::join_paths(directories) {
                            command.env("LD_LIBRARY_PATH", paths);
                        }
                    }
                }
                command
                    .kill_on_drop(true)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                #[cfg(windows)]
                command.creation_flags(0x08000000);
                match command.spawn() {
                    Ok(child) => self.provider = Some(child),
                    Err(e) => {
                        self.state.status.last_error = Some(format!("Bindings provider: {e}"))
                    }
                }
            }
        }
        self.state.status.bindings_provider_running = self
            .provider
            .as_mut()
            .is_some_and(|p| p.try_wait().is_ok_and(|status| status.is_none()));
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
        self.rpc_task.abort();
        self.feeder_task.abort();
        self.feeder_forward.abort();
    }
}
fn find_provider() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "SlimeVR-Bindings-Provider.exe"
    } else {
        "slimevr-bindings-provider"
    };
    let mut paths = vec![PathBuf::from(name)];
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            paths.push(parent.join(name));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|p| p.join(name)));
    }
    paths
        .into_iter()
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok())
}

pub fn source_key(source: &str, name: &str) -> String {
    if name.len() <= 96 {
        format!("{source}:{name}")
    } else {
        use sha2::{Digest, Sha256};
        format!("{source}:{:x}", Sha256::digest(name.as_bytes()))
    }
}
fn external(at: u64, kind: slimevr_core::EventKind) -> SceneInput {
    SceneInput::Input {
        event: slimevr_core::InputEvent { at_ms: at, kind },
    }
}
fn role_body(role: i32) -> Option<B> {
    Some(match role {
        1 => B::Hip,
        2 => B::LeftFoot,
        3 => B::RightFoot,
        4 => B::UpperChest,
        5 => B::LeftUpperLeg,
        6 => B::RightUpperLeg,
        7 => B::LeftUpperArm,
        8 => B::RightUpperArm,
        9 => B::LeftLowerArm,
        10 => B::RightLowerArm,
        _ => return None,
    })
}
