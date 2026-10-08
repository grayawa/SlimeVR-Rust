use super::{
    armature::{self, Armature},
    Settings,
};
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use slimevr_core::{
    pose::PoseSnapshot,
    skeleton::{BodyPosition as B, HeadPose},
    EventKind as E, InputEvent, Quaternion as Q, TrackerCapabilities, TrackerSample, Vector3 as V,
};
use std::collections::BTreeMap;
const VRC_ROLES: [(&str, &str, u8); 8] = [
    ("hip", "waist", 1),
    ("left_foot", "left_foot", 2),
    ("right_foot", "right_foot", 3),
    ("left_knee", "left_knee", 4),
    ("right_knee", "right_knee", 5),
    ("chest", "chest", 6),
    ("left_elbow", "left_elbow", 7),
    ("right_elbow", "right_elbow", 8),
];
#[derive(Clone, Copy)]
struct Raw {
    rotation: Q,
    position: Option<V>,
}
pub struct Output {
    pub vrc: Option<Vec<u8>>,
    pub vmc: Option<Vec<u8>>,
}
pub struct State {
    pub config: Settings,
    pub generation: u64,
    pub neck_height: f32,
    pub controller_arms: std::collections::BTreeSet<B>,
    raw: BTreeMap<String, Raw>,
    registered: BTreeMap<String, ()>,
    position_offset: V,
    post_position_offset: V,
    rotation_offset: Q,
    rotation_goal: Q,
    last_offset: u64,
    last_tick: u64,
    last_send: u64,
    last_reset: u64,
    yaw_offset: Q,
    input: Option<Armature>,
    output: Armature,
    vrm_height: f32,
}
impl State {
    pub fn new(config: Settings) -> Result<Self, String> {
        config.validate()?;
        let mut output = Armature::new(false);
        let vrm_height = config
            .vmc
            .vrm_json
            .as_deref()
            .map(|s| output.load_vrm(s))
            .transpose()?
            .unwrap_or(0.0);
        Ok(Self {
            config,
            generation: 1,
            neck_height: slimevr_core::autobone::skeleton_height(Default::default())
                - slimevr_core::skeleton::SkeletonConfig::default().neck_length,
            controller_arms: Default::default(),
            raw: Default::default(),
            registered: Default::default(),
            position_offset: V::ZERO,
            post_position_offset: V::ZERO,
            rotation_offset: Q::IDENTITY,
            rotation_goal: Q::IDENTITY,
            last_offset: 0,
            last_tick: 0,
            last_send: 0,
            last_reset: 0,
            yaw_offset: Q::IDENTITY,
            input: None,
            output,
            vrm_height,
        })
    }
    pub fn reconfigure(&mut self, config: Settings, at: u64) -> Result<Vec<InputEvent>, String> {
        if self.config == config {
            return Ok(vec![]);
        }
        let disconnected = self
            .registered
            .keys()
            .map(|key| InputEvent {
                at_ms: at,
                kind: E::SensorState {
                    device_key: key.clone(),
                    sensor_id: 0,
                    status: slimevr_core::SensorStatus::Disconnected,
                },
            })
            .collect();
        let generation = self.generation + 1;
        let mut next = Self::new(config)?;
        next.generation = generation;
        next.last_reset = self.last_reset;
        *self = next;
        Ok(disconnected)
    }
    pub fn receive(
        &mut self,
        port: u16,
        bytes: &[u8],
        at: u64,
        head: Option<HeadPose>,
    ) -> Result<Vec<InputEvent>, String> {
        bounded(bytes, 0)?;
        let (remaining, packet) =
            rosc::decoder::decode_udp(bytes).map_err(|e| format!("invalid OSC packet: {e}"))?;
        if !remaining.is_empty() {
            return Err("trailing OSC bytes".into());
        }
        let mut messages = vec![];
        flatten(packet, &mut messages);
        if messages.len() > 256 {
            return Err("too many OSC messages".into());
        }
        let mut events = vec![];
        for m in messages {
            if m.addr.len() > 256 || m.args.len() > 64 {
                return Err("OSC message limits exceeded".into());
            }
            if self.config.vrc.endpoint.enabled && port == self.config.vrc.endpoint.port_in {
                self.vrc(&m, at, head, &mut events)?;
            }
            if self.config.vmc.endpoint.enabled && port == self.config.vmc.endpoint.port_in {
                self.vmc(&m, at, &mut events)?;
            }
        }
        Ok(events)
    }
    fn register(
        &mut self,
        source: &str,
        name: &str,
        body: Option<B>,
        reset: bool,
        at: u64,
        events: &mut Vec<InputEvent>,
    ) -> Result<String, String> {
        let key = crate::steamvr::source_key(source, name);
        if !self.registered.contains_key(&key) {
            if self.registered.len() >= 128 {
                return Err("OSC tracker limit reached".into());
            }
            self.registered.insert(key.clone(), ());
            self.raw.insert(
                key.clone(),
                Raw {
                    rotation: Q::IDENTITY,
                    position: None,
                },
            );
            events.push(InputEvent {
                at_ms: at,
                kind: E::ExternalTracker {
                    device_key: key.clone(),
                    sensor_id: 0,
                    source: source.into(),
                    name: name.into(),
                    body,
                    capabilities: TrackerCapabilities {
                        allow_reset: reset,
                        allow_mounting: false,
                        allow_filter: false,
                        is_imu: false,
                    },
                },
            });
            events.push(InputEvent {
                at_ms: at,
                kind: E::DeviceConnected {
                    device_key: key.clone(),
                    address: source.into(),
                    firmware: None,
                    session: self.generation,
                    preserve_calibration: false,
                },
            });
        }
        Ok(key)
    }
    fn sample(&self, source: &str, key: &str, at: u64, events: &mut Vec<InputEvent>) {
        let raw = self.raw[key];
        events.push(InputEvent {
            at_ms: at,
            kind: E::Sample {
                sample: TrackerSample {
                    source: source.into(),
                    device_key: key.into(),
                    sensor_id: 0,
                    session: self.generation,
                    packet_sequence: at as i64,
                    received_at_ms: at,
                    socket_received_at_ms: None,
                    sensor_timestamp_us: None,
                    packet_rotation: Some(raw.rotation),
                    server_rotation: Some(raw.rotation),
                    packet_acceleration: None,
                    server_acceleration: None,
                    position: raw.position,
                    compatibility_fallback: false,
                },
            },
        });
    }
    fn vrc(
        &mut self,
        m: &OscMessage,
        at: u64,
        head: Option<HeadPose>,
        events: &mut Vec<InputEvent>,
    ) -> Result<(), String> {
        let path: Vec<_> = m.addr.split('/').collect();
        if path.len() != 5 || path[1] != "tracking" {
            return Ok(());
        }
        if path[2] == "vrsystem" && path[4] == "pose" {
            let (body, name) = match path[3] {
                "head" => (B::Head, "VRChat head"),
                "leftwrist" => (B::LeftHand, "VRChat left hand"),
                "rightwrist" => (B::RightHand, "VRChat right hand"),
                _ => return Ok(()),
            };
            let v = values(m, 6)?;
            let key = self.register("vrchat", name, Some(body), body != B::Head, at, events)?;
            self.raw.insert(
                key.clone(),
                Raw {
                    position: Some(V::new(v[0], v[1], -v[2])),
                    rotation: vrc_rotation(&v[3..]),
                },
            );
            self.sample("vrchat", &key, at, events);
        } else if path[2] == "trackers" && matches!(path[4], "position" | "rotation") {
            let v = values(m, 3)?;
            if path[3] == "head" {
                if path[4] == "position" {
                    self.position_offset = V::new(v[0], v[1], -v[2]);
                    if let Some(p) = head.and_then(|h| h.position) {
                        self.post_position_offset = p;
                    }
                } else {
                    self.rotation_goal = head
                        .map(|h| h.rotation.project(V::UP).unit())
                        .unwrap_or(Q::IDENTITY)
                        * vrc_rotation(&v).inv();
                    if at.saturating_sub(self.last_offset) > 300 {
                        self.rotation_offset = self.rotation_goal;
                    }
                    self.last_offset = at;
                }
            } else {
                let id: u16 = path[3].parse().map_err(|_| "invalid OSC tracker number")?;
                if !(1..=255).contains(&id) {
                    return Err("OSC tracker number outside 1..255".into());
                }
                let key =
                    self.register("osc", &format!("OSC Tracker #{id}"), None, true, at, events)?;
                let raw = self.raw.get_mut(&key).unwrap();
                if path[4] == "position" {
                    raw.position = Some(
                        self.rotation_offset
                            .rotate(V::new(v[0], v[1], -v[2]) - self.position_offset)
                            + self.post_position_offset,
                    );
                } else {
                    raw.rotation = self.rotation_offset
                        * vrc_rotation(&v)
                        * Q::rotation_y(std::f32::consts::PI);
                }
                self.sample("osc", &key, at, events);
            }
        }
        Ok(())
    }
    fn vmc(&mut self, m: &OscMessage, at: u64, events: &mut Vec<InputEvent>) -> Result<(), String> {
        if !matches!(
            m.addr.as_str(),
            "/VMC/Ext/Bone/Pos"
                | "/VMC/Ext/Hmd/Pos"
                | "/VMC/Ext/Con/Pos"
                | "/VMC/Ext/Tra/Pos"
                | "/VMC/Ext/Root/Pos"
        ) {
            return Ok(());
        }
        if m.args.len() != 8 {
            return Err("VMC pose needs name and seven floats".into());
        }
        let Some(OscType::String(name)) = m.args.first() else {
            return Err("VMC tracker name must be a string".into());
        };
        if name.len() > 200 || name.is_empty() {
            return Err("invalid VMC tracker name".into());
        }
        let v = numbers(&m.args[1..])?;
        let p = V::new(v[0], v[1], -v[2]);
        let q = Q::new(-v[6], v[3], v[4], -v[5]);
        if !q.is_rotation() {
            return Err("invalid VMC rotation".into());
        }
        match m.addr.as_str() {
            "/VMC/Ext/Root/Pos" => {
                if let Some(input) = &mut self.input {
                    input.root(p, q);
                }
            }
            "/VMC/Ext/Bone/Pos" => {
                if let Some(b) = Armature::bone(name) {
                    let key = self.register(
                        "vmc",
                        &format!("VMC-Bone-{name}"),
                        Some(b.body),
                        false,
                        at,
                        events,
                    )?;
                    let input = self.input.get_or_insert_with(|| Armature::new(true));
                    input.set_local(b, q);
                    let rotation = self.yaw_offset * input.global(b);
                    self.raw.insert(
                        key.clone(),
                        Raw {
                            rotation,
                            position: None,
                        },
                    );
                    self.sample("vmc", &key, at, events);
                }
            }
            _ => {
                if name.starts_with("human://") {
                    return Ok(());
                }
                let key = self.register(
                    "vmc",
                    &format!("VMC-Tracker-{name}"),
                    None,
                    true,
                    at,
                    events,
                )?;
                self.raw.insert(
                    key.clone(),
                    Raw {
                        rotation: q,
                        position: Some(p),
                    },
                );
                self.sample("vmc", &key, at, events);
            }
        }
        Ok(())
    }
    pub fn output(
        &mut self,
        pose: &PoseSnapshot,
        at: u64,
        head: Option<HeadPose>,
    ) -> Result<Output, String> {
        let dt = at.saturating_sub(self.last_tick) as f32 / 1000.0;
        self.last_tick = at;
        self.rotation_offset = self.rotation_offset.interp_r(self.rotation_goal, 0.5 * dt);
        if let Some(input) = &mut self.input {
            input.update();
        }
        let reset = pose.reset_count != self.last_reset;
        self.last_reset = pose.reset_count;
        if reset {
            self.yaw_offset = head
                .map(|h| h.rotation.project(V::UP).unit())
                .unwrap_or(Q::IDENTITY);
        }
        let vrc = if self.config.vrc.endpoint.enabled {
            let mut messages = vec![];
            for (name, role, id) in VRC_ROLES {
                if self.config.vrc.trackers.get(role) == Some(&true) {
                    if let Some(p) = pose.skeleton.computed.get(name) {
                        messages.push(message(
                            &format!("/tracking/trackers/{id}/position"),
                            floats(&[p.position.x, p.position.y, -p.position.z]),
                        ));
                        let e = Q::new(p.rotation.w, -p.rotation.x, -p.rotation.y, p.rotation.z)
                            .euler_yxz();
                        messages.push(message(
                            &format!("/tracking/trackers/{id}/rotation"),
                            floats(&[e.x.to_degrees(), e.y.to_degrees(), e.z.to_degrees()]),
                        ));
                    }
                }
            }
            if let Some(p) = pose.skeleton.computed.get("head") {
                messages.push(message(
                    "/tracking/trackers/head/position",
                    floats(&[p.position.x, p.position.y, -p.position.z]),
                ));
            }
            if reset {
                if let Some(h) = head {
                    messages.push(message(
                        "/tracking/trackers/head/rotation",
                        floats(&[0.0, -h.rotation.euler_yxz().y.to_degrees(), 0.0]),
                    ));
                }
            }
            Some(bundle(messages)?)
        } else {
            None
        };
        let vmc = if self.config.vmc.endpoint.enabled && at.saturating_sub(self.last_send) > 3 {
            self.last_send = at;
            let mut messages = vec![message("/VMC/Ext/T", floats(&[at as f32 / 1000.0]))];
            if !pose.trackers.is_empty() || head.is_some() {
                messages.push(message("/VMC/Ext/OK", vec![OscType::Int(1)]));
                messages.push(message(
                    "/VMC/Ext/Root/Pos",
                    transform("root", V::ZERO, Q::IDENTITY),
                ));
                for b in armature::BONES {
                    let source = if self.config.vmc.mirror_tracking {
                        armature::BONES
                            .iter()
                            .find(|other| other.body == armature::opposite(b.body))
                            .unwrap_or(b)
                    } else {
                        b
                    };
                    if let Some(bone) = pose.skeleton.bones.get(source.slime_bone) {
                        let mut q = bone.rotation * bone.rotation_offset.inv();
                        if self.config.vmc.mirror_tracking {
                            q = Q::new(q.w, q.x, -q.y, -q.z);
                        }
                        self.output.set_global(b, q);
                    }
                }
                if !self.config.vmc.anchor_hip && self.vrm_height > 0.0 {
                    if let Some(neck) = pose.skeleton.bones.get("neck") {
                        let neck_height = self.neck_height;
                        if neck_height.abs() > 1e-4 {
                            self.output
                                .anchor_head(neck.tail * (self.vrm_height / neck_height));
                        }
                    }
                }
                self.output.update();
                for b in armature::BONES {
                    let source_body = if self.config.vmc.mirror_tracking {
                        armature::opposite(b.body)
                    } else {
                        b.body
                    };
                    if (b.body.is_left_finger() || b.body.is_right_finger())
                        && !pose.trackers.iter().any(|p| {
                            if source_body.is_left_finger() {
                                p.body.is_left_finger()
                            } else {
                                p.body.is_right_finger()
                            }
                        })
                    {
                        continue;
                    }
                    let side = if source_body.is_left_arm() || source_body.is_left_finger() {
                        Some(B::LeftHand)
                    } else if source_body.is_right_arm() || source_body.is_right_finger() {
                        Some(B::RightHand)
                    } else {
                        None
                    };
                    if side.is_some_and(|hand| self.controller_arms.contains(&hand)) {
                        continue;
                    }
                    messages.push(message(
                        "/VMC/Ext/Bone/Pos",
                        transform(
                            b.name,
                            self.output.local_translation(b),
                            self.output.local_rotation(b),
                        ),
                    ));
                }
            }
            if pose.skeleton.world_anchor_present {
                for (name, p) in &pose.skeleton.computed {
                    let (addr, role) = match name.as_str() {
                        "head" => ("/VMC/Ext/Hmd/Pos", "HEAD"),
                        "left_hand" => ("/VMC/Ext/Con/Pos", "LEFT_HAND"),
                        "right_hand" => ("/VMC/Ext/Con/Pos", "RIGHT_HAND"),
                        _ => (
                            "/VMC/Ext/Tra/Pos",
                            match name.as_str() {
                                "hip" => "WAIST",
                                other => other,
                            },
                        ),
                    };
                    messages.push(message(
                        addr,
                        transform(
                            &format!("human://{}", role.to_ascii_uppercase()),
                            p.position,
                            p.rotation,
                        ),
                    ));
                }
            }
            Some(bundle(messages)?)
        } else {
            None
        };
        Ok(Output { vrc, vmc })
    }
}
fn bounded(bytes: &[u8], depth: u8) -> Result<(), String> {
    if bytes.len() > 8192 || depth > 8 || bytes.len() < 4 {
        return Err("OSC packet size or depth limit exceeded".into());
    }
    if bytes.starts_with(b"#bundle\0") {
        if bytes.len() < 16 {
            return Err("truncated OSC bundle".into());
        }
        let mut offset = 16;
        while offset < bytes.len() {
            let length = bytes
                .get(offset..offset + 4)
                .ok_or("truncated OSC element length")?;
            let len = u32::from_be_bytes(length.try_into().unwrap()) as usize;
            offset += 4;
            let element = bytes
                .get(offset..offset.saturating_add(len))
                .ok_or("truncated OSC element")?;
            bounded(element, depth + 1)?;
            offset += len;
        }
    }
    Ok(())
}
fn flatten(packet: OscPacket, messages: &mut Vec<OscMessage>) {
    match packet {
        OscPacket::Message(m) => messages.push(m),
        OscPacket::Bundle(b) => {
            for p in b.content {
                flatten(p, messages);
            }
        }
    }
}
fn numbers(args: &[OscType]) -> Result<Vec<f32>, String> {
    args.iter()
        .map(|a| {
            let v = match a {
                OscType::Float(v) => *v,
                OscType::Double(v) => *v as f32,
                OscType::Int(v) => *v as f32,
                _ => return Err("OSC argument must be numeric".into()),
            };
            if !v.is_finite() {
                return Err("OSC number must be finite".into());
            }
            Ok(v)
        })
        .collect()
}
fn values(m: &OscMessage, len: usize) -> Result<Vec<f32>, String> {
    if m.args.len() != len {
        return Err(format!("{} needs {len} numeric values", m.addr));
    }
    numbers(&m.args)
}
fn vrc_rotation(v: &[f32]) -> Q {
    let q = Q::from_euler_yxz(V::new(
        v[0].to_radians(),
        v[1].to_radians(),
        v[2].to_radians(),
    ));
    Q::new(q.w, -q.x, -q.y, q.z)
}
fn floats(v: &[f32]) -> Vec<OscType> {
    v.iter().copied().map(OscType::Float).collect()
}
fn message(addr: &str, args: Vec<OscType>) -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: addr.into(),
        args,
    })
}
fn bundle(content: Vec<OscPacket>) -> Result<Vec<u8>, String> {
    rosc::encoder::encode(&OscPacket::Bundle(OscBundle {
        timetag: (0, 1).into(),
        content,
    }))
    .map_err(|e| e.to_string())
}
fn transform(name: &str, p: V, q: Q) -> Vec<OscType> {
    let mut a = vec![OscType::String(name.into())];
    a.extend(floats(&[p.x, p.y, -p.z, q.x, q.y, -q.z, -q.w]));
    a
}
