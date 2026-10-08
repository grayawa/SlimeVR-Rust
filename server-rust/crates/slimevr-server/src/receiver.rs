//! Single-owner receiver state. The caller supplies a monotonic clock and transports effects.
use crate::protocol::{self, Handshake, Packet, SensorInfo};
use serde::{Deserialize, Serialize};
use slimevr_core::{
    EventKind, InputEvent, Quaternion, SensorStatus, Timed, TrackerSample, Vector3,
};
use std::{collections::BTreeMap, net::SocketAddr};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiverConfig {
    pub accept_new_devices: bool,
    pub allowed_macs: Vec<String>,
    pub max_devices: usize,
    pub transport_timeout_ms: u64,
    pub sensor_timeout_ms: u64,
    pub disconnect_ms: u64,
    pub maintenance_ms: u64,
}
impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            accept_new_devices: false,
            allowed_macs: Vec::new(),
            max_devices: 64,
            transport_timeout_ms: 1000,
            sensor_timeout_ms: 2000,
            disconnect_ms: 5000,
            maintenance_ms: 500,
        }
    }
}
impl ReceiverConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1024).contains(&self.max_devices) || self.allowed_macs.len() > 1024 {
            return Err("device limit must be 1..1024; at most 1024 allowed MACs".into());
        }
        if self.maintenance_ms == 0
            || self.transport_timeout_ms == 0
            || self.sensor_timeout_ms == 0
            || self.disconnect_ms < self.sensor_timeout_ms
        {
            return Err("invalid receiver timeouts".into());
        }
        for mac in &self.allowed_macs {
            normalize_mac(mac)?;
        }
        Ok(())
    }
}

pub fn normalize_mac(value: &str) -> Result<String, String> {
    let hex: String = value.chars().filter(|c| *c != ':' && *c != '-').collect();
    if hex.len() != 12 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("MAC must contain six hexadecimal bytes".into());
    }
    if hex.bytes().all(|b| b == b'0') {
        return Err("all-zero MAC cannot identify a device".into());
    }
    Ok((0..6)
        .map(|i| hex[i * 2..i * 2 + 2].to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(":"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counters {
    pub received: u64,
    pub accepted: u64,
    pub malformed: u64,
    pub unknown_sources: u64,
    pub admission_denied: u64,
    pub sequence_rejected: u64,
    /// Diagnostic only: UDP gaps cannot distinguish loss, reordering or sender behavior.
    pub sequence_gaps: u64,
    pub ignored_packets: u64,
    pub samples: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SensorState {
    pub capabilities: slimevr_core::TrackerCapabilities,
    pub magnetic_vector: Option<Timed<Vector3>>,
    pub button: u8,
    pub info: SensorInfo,
    pub status: SensorStatus,
    pub last_alive_ms: u64,
    pub rotation: Option<Timed<Quaternion>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_rotation_timing: Option<ReceiveTiming>,
    pub acceleration: Option<Timed<Vector3>>,
    pub position: Option<Timed<Vector3>>,
    pub temperature: Option<Timed<f32>>,
    pub flex: Option<Timed<f32>>,
    pub calibration: Option<u8>,
    pub error_code: Option<Timed<u8>>,
    pub samples: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ReceiveTiming {
    pub received_at_ms: u64,
    pub processed_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceState {
    pub display_name: Option<String>,
    pub origin: Origin,
    pub sleep_at: Option<u64>,
    pub battery_runtime: Option<u64>,
    pub firmware_date: Option<String>,
    pub packet_loss: Option<f32>,
    pub packets_received: Option<i32>,
    pub packets_lost: Option<i32>,
    pub key: String,
    pub address: SocketAddr,
    pub handshake: Handshake,
    pub session: u64,
    #[serde(skip)]
    pub config_acks: BTreeMap<(u8, u16), u64>,
    #[serde(skip)]
    config_requests: BTreeMap<(u8, u16), bool>,
    #[serde(skip)]
    ack_counter: u64,
    pub last_alive_ms: u64,
    pub last_sequence: i64,
    pub transport_timed_out: bool,
    pub sensors: BTreeMap<u8, SensorState>,
    pub features: Vec<u8>,
    pub features_available: bool,
    pub battery_voltage: Option<Timed<f32>>,
    pub battery_fraction: Option<Timed<f32>>,
    pub rssi: Option<Timed<i16>>,
    pub half_rtt_ms: Option<Timed<u64>>,
    #[serde(skip)]
    pending_ping: Option<(i32, u64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub device_key: String,
    pub sensor_id: u8,
    pub config_type: u16,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outbound {
    pub to: SocketAddr,
    pub bytes: Vec<u8>,
}
#[derive(Default, Debug)]
pub struct Effects {
    pub events: Vec<InputEvent>,
    pub outbound: Vec<Outbound>,
}
impl Effects {
    fn event(&mut self, at_ms: u64, kind: EventKind) {
        self.events.push(InputEvent { at_ms, kind });
    }
    fn send(&mut self, to: SocketAddr, bytes: Vec<u8>) {
        self.outbound.push(Outbound { to, bytes });
    }
    fn ignore(&mut self, at_ms: u64, address: SocketAddr, id: u32, reason: &str) {
        self.event(
            at_ms,
            EventKind::Ignored {
                address: address.to_string(),
                packet_id: id,
                reason: reason.into(),
            },
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Udp,
    Hid,
    SteamVr,
    Osc,
    Vrchat,
    Vmc,
}
mod external;
mod hid;
pub struct Receiver {
    pub config: ReceiverConfig,
    pub devices: BTreeMap<String, DeviceState>,
    pub counters: Counters,
    hid_sources: BTreeMap<(String, u64), BTreeMap<u8, String>>,
    addresses: BTreeMap<SocketAddr, String>,
    last_maintenance_ms: u64,
    ping_id: i32,
    // Per-address approval notifications, capped at 64 recent sources.
    last_pending_event_ms: BTreeMap<SocketAddr, u64>,
}
impl Receiver {
    pub fn config_command(&mut self, command: &DeviceConfig) -> Result<Effects, String> {
        let d = self
            .devices
            .get_mut(&command.device_key)
            .ok_or("unknown UDP device")?;
        if d.origin == Origin::Hid {
            if command.config_type != 1 {
                return Err("unsupported HID configuration".into());
            }
            let sensor = d
                .sensors
                .get_mut(&command.sensor_id)
                .ok_or("unknown HID sensor")?;
            let flags = sensor
                .info
                .config
                .filter(|flags| flags & 2 != 0)
                .ok_or("HID tracker has no magnetometer")?;
            sensor.info.config = Some((flags & !1) | u16::from(command.enabled));
            d.ack_counter = d.ack_counter.saturating_add(1);
            d.config_acks
                .insert((command.sensor_id, command.config_type), d.ack_counter);
            return Ok(Effects::default());
        }
        if d.origin != Origin::Udp {
            return Err("configuration commands are unavailable for this input source".into());
        }
        if d.transport_timed_out || !d.sensors.contains_key(&command.sensor_id) {
            return Err("tracker is offline or unknown".into());
        }
        if d.config_requests.len() >= 256 {
            return Err("too many outstanding device configurations".into());
        }
        d.config_requests
            .insert((command.sensor_id, command.config_type), command.enabled);
        let [hi, lo] = command.config_type.to_be_bytes();
        Ok(Effects {
            outbound: vec![Outbound {
                to: d.address,
                bytes: protocol::header(
                    25,
                    &[command.sensor_id, hi, lo, u8::from(command.enabled)],
                ),
            }],
            ..Default::default()
        })
    }

    pub fn new(mut config: ReceiverConfig) -> Result<Self, String> {
        config.validate()?;
        config.allowed_macs = config
            .allowed_macs
            .iter()
            .map(|m| normalize_mac(m))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            config,
            devices: BTreeMap::new(),
            counters: Counters::default(),
            hid_sources: BTreeMap::new(),
            addresses: BTreeMap::new(),
            last_maintenance_ms: 0,
            ping_id: 0,
            last_pending_event_ms: BTreeMap::new(),
        })
    }

    pub fn forget_device(&mut self, key: &str) {
        self.devices.remove(key);
        self.addresses.retain(|_, device| device != key);
    }
    pub fn receive(&mut self, from: SocketAddr, bytes: &[u8], at_ms: u64) -> Effects {
        self.receive_parsed(from, protocol::parse(bytes), at_ms, None)
    }
    pub fn receive_timed(
        &mut self,
        from: SocketAddr,
        bytes: &[u8],
        at_ms: u64,
        received_at_ms: Option<u64>,
    ) -> Effects {
        self.receive_parsed(from, protocol::parse(bytes), at_ms, received_at_ms)
    }
    pub(crate) fn receive_parsed(
        &mut self,
        from: SocketAddr,
        parsed: Result<protocol::Datagram, protocol::ParseError>,
        at_ms: u64,
        received_at_ms: Option<u64>,
    ) -> Effects {
        let mut fx = Effects::default();
        self.counters.received += 1;
        let datagram = match parsed {
            Ok(p) => p,
            Err(e) => {
                self.counters.malformed += 1;
                fx.event(
                    at_ms,
                    EventKind::Rejected {
                        address: from.to_string(),
                        reason: e.to_string(),
                    },
                );
                return fx;
            }
        };
        // Only a standalone, successfully parsed handshake can reset a session.
        let key = if let [Packet::Handshake { info }] = datagram.packets.as_slice() {
            if datagram.packet_id != 3 {
                self.counters.ignored_packets += 1;
                fx.ignore(
                    at_ms,
                    from,
                    datagram.packet_id,
                    "bundled handshake is unsupported",
                );
                return fx;
            }
            match self.connect(from, info.clone(), at_ms, &mut fx) {
                Some(k) => k,
                None => return fx,
            }
        } else {
            match self.addresses.get(&from) {
                Some(k) => k.clone(),
                None => {
                    self.counters.unknown_sources += 1;
                    fx.ignore(at_ms, from, datagram.packet_id, "handshake required");
                    return fx;
                }
            }
        };
        let device = self.devices.get_mut(&key).unwrap();
        let is_handshake = datagram.packet_id == 3;
        if !is_handshake && datagram.sequence != 0 && datagram.sequence <= device.last_sequence {
            self.counters.sequence_rejected += 1;
            fx.event(
                at_ms,
                EventKind::Rejected {
                    address: from.to_string(),
                    reason: "duplicate or out-of-order sequence".into(),
                },
            );
            return fx;
        }
        if !is_handshake && datagram.sequence > device.last_sequence && device.last_sequence > 0 {
            self.counters.sequence_gaps = self.counters.sequence_gaps.saturating_add(
                (datagram.sequence as u64)
                    .saturating_sub(device.last_sequence as u64)
                    .saturating_sub(1),
            );
        }
        // Handshake establishes sequence zero, as reset by Java's setUpNewDevice.
        device.last_sequence = if is_handshake { 0 } else { datagram.sequence };
        device.last_alive_ms = at_ms;
        if device.transport_timed_out {
            device.transport_timed_out = false;
            fx.event(
                at_ms,
                EventKind::TransportState {
                    device_key: key.clone(),
                    timed_out: false,
                },
            );
        }
        for sensor in device.sensors.values_mut() {
            sensor.last_alive_ms = at_ms;
        }
        self.counters.accepted += 1;
        for warning in datagram.warnings {
            fx.event(
                at_ms,
                EventKind::CompatibilityFallback {
                    address: from.to_string(),
                    reason: warning,
                },
            );
        }
        for packet in datagram.packets {
            process_packet(
                device,
                packet,
                datagram.sequence,
                at_ms,
                received_at_ms,
                &mut fx,
                &mut self.counters,
            );
        }
        fx
    }

    fn connect(
        &mut self,
        from: SocketAddr,
        info: Handshake,
        at_ms: u64,
        fx: &mut Effects,
    ) -> Option<String> {
        let allowed = self.config.accept_new_devices
            || info
                .mac
                .as_ref()
                .is_some_and(|mac| self.config.allowed_macs.contains(mac));
        if !allowed {
            self.counters.admission_denied += 1;
            self.last_pending_event_ms
                .retain(|_, t| at_ms.saturating_sub(*t) < 1000);
            if !self.last_pending_event_ms.contains_key(&from)
                && self.last_pending_event_ms.len() < 64
            {
                self.last_pending_event_ms.insert(from, at_ms);
                fx.event(
                    at_ms,
                    EventKind::DevicePending {
                        address: from.to_string(),
                        mac: info.mac,
                    },
                );
            }
            return None;
        }
        let key = info.mac.clone().unwrap_or_else(|| format!("udp:{from}"));
        if !self.devices.contains_key(&key) && self.devices.len() >= self.config.max_devices {
            self.counters.admission_denied += 1;
            fx.event(
                at_ms,
                EventKind::Rejected {
                    address: from.to_string(),
                    reason: "device limit reached".into(),
                },
            );
            return None;
        }
        // Java reuses the tracker and its reset corrections on a transport retry.
        // A missed keepalive under host load can trigger this without restarting the IMU.
        let preserve_calibration = self.devices.contains_key(&key);
        // Remove the former endpoint on MAC-stable reconnect; retain the device identity.
        let session = self
            .devices
            .get(&key)
            .map_or(1, |d| d.session.saturating_add(1));
        if let Some(old) = self.devices.get(&key) {
            if self.addresses.get(&old.address) == Some(&key) {
                self.addresses.remove(&old.address);
            }
        }
        // A different MAC on the same endpoint displaces the old endpoint mapping.
        if let Some(displaced) = self.addresses.remove(&from) {
            if let Some(d) = self.devices.get_mut(&displaced) {
                d.transport_timed_out = true;
                fx.event(
                    at_ms,
                    EventKind::TransportState {
                        device_key: displaced,
                        timed_out: true,
                    },
                );
            }
        }
        let mut device = DeviceState {
            display_name: None,
            origin: Origin::Udp,
            sleep_at: None,
            battery_runtime: None,
            firmware_date: None,
            packet_loss: None,
            packets_received: None,
            packets_lost: None,
            key: key.clone(),
            address: from,
            handshake: info.clone(),
            session,
            config_acks: BTreeMap::new(),
            config_requests: BTreeMap::new(),
            ack_counter: 0,
            last_alive_ms: at_ms,
            last_sequence: 0,
            transport_timed_out: false,
            sensors: BTreeMap::new(),
            features: Vec::new(),
            features_available: false,
            battery_voltage: None,
            battery_fraction: None,
            rssi: None,
            half_rtt_ms: None,
            pending_ping: None,
        };
        // Legacy firmware never announces SensorInfo. Current firmware must announce it.
        if info.protocol_version < 9 || info.firmware.as_deref().is_none_or(str::is_empty) {
            register_sensor(
                &mut device,
                SensorInfo {
                    sensor_id: 0,
                    status: 1,
                    imu_type: info.imu_type as u8,
                    config: None,
                    rest_calibrated: None,
                    body_position: None,
                    data_type: 0,
                },
                at_ms,
                fx,
            );
        }
        fx.event(
            at_ms,
            EventKind::DeviceConnected {
                device_key: key.clone(),
                address: from.to_string(),
                firmware: info.firmware,
                session,
                preserve_calibration,
            },
        );
        fx.send(from, protocol::handshake_response());
        fx.send(from, protocol::header(22, &[protocol::SERVER_FEATURES]));
        self.addresses.insert(from, key.clone());
        self.devices.insert(key.clone(), device);
        Some(key)
    }

    pub fn tick(&mut self, at_ms: u64) -> Effects {
        let mut fx = Effects::default();
        for device in self.devices.values_mut() {
            if device.origin == Origin::Hid {
                if device.sleep_at.is_some_and(|sleep| at_ms >= sleep) {
                    device.sleep_at = None;
                    for sensor in device.sensors.values_mut() {
                        if sensor.status != SensorStatus::TimedOut {
                            sensor.status = SensorStatus::TimedOut;
                            fx.event(
                                at_ms,
                                EventKind::SensorState {
                                    device_key: device.key.clone(),
                                    sensor_id: sensor.info.sensor_id,
                                    status: SensorStatus::TimedOut,
                                },
                            );
                        }
                    }
                }
                continue;
            }
            let timeout = (device.origin == Origin::Udp
                && self.addresses.get(&device.address) != Some(&device.key))
                || at_ms.saturating_sub(device.last_alive_ms) > self.config.transport_timeout_ms;
            if timeout != device.transport_timed_out {
                device.transport_timed_out = timeout;
                fx.event(
                    at_ms,
                    EventKind::TransportState {
                        device_key: device.key.clone(),
                        timed_out: timeout,
                    },
                );
            }
            for sensor in device.sensors.values_mut() {
                let age = at_ms.saturating_sub(sensor.last_alive_ms);
                let desired = if age > self.config.disconnect_ms {
                    Some(SensorStatus::Disconnected)
                } else if age > self.config.sensor_timeout_ms {
                    Some(SensorStatus::TimedOut)
                } else {
                    None
                };
                if let Some(status) = desired {
                    if status != sensor.status {
                        sensor.status = status;
                        fx.event(
                            at_ms,
                            EventKind::SensorState {
                                device_key: device.key.clone(),
                                sensor_id: sensor.info.sensor_id,
                                status,
                            },
                        );
                    }
                }
            }
        }
        if at_ms.saturating_sub(self.last_maintenance_ms) >= self.config.maintenance_ms {
            self.last_maintenance_ms = at_ms;
            self.ping_id = self.ping_id.wrapping_add(1);
            for device in self.devices.values_mut() {
                // Retired endpoints must not receive traffic intended for their replacement.
                if device.origin != Origin::Udp
                    || self.addresses.get(&device.address) != Some(&device.key)
                {
                    continue;
                }
                fx.send(device.address, protocol::header(1, &[]));
                fx.send(
                    device.address,
                    protocol::header(10, &self.ping_id.to_be_bytes()),
                );
                device.pending_ping = Some((self.ping_id, at_ms));
                if !device.features_available {
                    fx.send(
                        device.address,
                        protocol::header(22, &[protocol::SERVER_FEATURES]),
                    );
                }
            }
        }
        fx
    }

    pub fn needs_discovery(&self) -> bool {
        !self.devices.values().any(|d| {
            d.origin == Origin::Udp
                && !d.transport_timed_out
                && d.sensors.values().any(|s| s.status == SensorStatus::Ok)
        })
    }

    pub fn snapshot(&self, at_ms: u64) -> serde_json::Value {
        let freshness: Vec<_> = self
            .devices
            .values()
            .flat_map(|d| {
                d.sensors.values().map(move |s| {
                    let processed_age = s
                        .rotation
                        .as_ref()
                        .map(|q| at_ms.saturating_sub(q.received_at_ms));
                    let age = s.udp_rotation_timing.map(|t| at_ms.saturating_sub(t.received_at_ms)).or(processed_age);
                    let mut value = serde_json::json!({"device_key": d.key, "sensor_id": s.info.sensor_id,
                        "pose_age_ms": age, "pose_stale": age.map(|a| a > self.config.sensor_timeout_ms)});
                    if let Some(t) = s.udp_rotation_timing {
                        value["received_at_ms"] = t.received_at_ms.into();
                        value["processed_at_ms"] = t.processed_at_ms.into();
                        value["pose_processing_age_ms"] = processed_age.into();
                        value["pose_queue_delay_ms"] = t.processed_at_ms.saturating_sub(t.received_at_ms).into();
                    }
                    value
                })
            })
            .collect();
        serde_json::json!({"type":"snapshot", "at_ms":at_ms, "counters":self.counters, "devices":self.devices, "freshness":freshness})
    }
}

fn register_sensor(device: &mut DeviceState, info: SensorInfo, at_ms: u64, fx: &mut Effects) {
    let status = match info.status {
        0 => SensorStatus::Disconnected,
        1 => SensorStatus::Ok,
        2 => SensorStatus::Error,
        _ => return,
    };
    fx.event(
        at_ms,
        EventKind::SensorMetadata {
            device_key: device.key.clone(),
            sensor_id: info.sensor_id,
            imu_type: info.imu_type,
            data_type: info.data_type,
            magnetometer_enabled: info.config.is_some_and(|flags| flags & 3 == 3),
        },
    );
    if let Some(sensor) = device.sensors.get_mut(&info.sensor_id) {
        sensor.info = info;
        sensor.last_alive_ms = at_ms;
        if sensor.status != status {
            sensor.status = status;
            fx.event(
                at_ms,
                EventKind::SensorState {
                    device_key: device.key.clone(),
                    sensor_id: sensor.info.sensor_id,
                    status,
                },
            );
        }
    } else {
        fx.event(
            at_ms,
            EventKind::SensorRegistered {
                device_key: device.key.clone(),
                sensor_id: info.sensor_id,
                status,
            },
        );
        device.sensors.insert(
            info.sensor_id,
            SensorState {
                capabilities: Default::default(),
                magnetic_vector: None,
                button: 0,
                info,
                status,
                last_alive_ms: at_ms,
                rotation: None,
                udp_rotation_timing: None,
                acceleration: None,
                position: None,
                temperature: None,
                flex: None,
                calibration: None,
                error_code: None,
                samples: 0,
            },
        );
    }
}

fn process_packet(
    d: &mut DeviceState,
    p: Packet,
    sequence: i64,
    at: u64,
    received_at_ms: Option<u64>,
    fx: &mut Effects,
    counters: &mut Counters,
) {
    let mut sample = TrackerSample {
        source: "udp".into(),
        device_key: d.key.clone(),
        sensor_id: 0,
        session: d.session,
        packet_sequence: sequence,
        received_at_ms: at,
        socket_received_at_ms: received_at_ms,
        sensor_timestamp_us: None,
        packet_rotation: None,
        server_rotation: None,
        packet_acceleration: None,
        server_acceleration: None,
        position: None,
        compatibility_fallback: false,
    };
    let ignored = match p {
        Packet::Handshake { .. } | Packet::Heartbeat => None,
        Packet::SensorInfo { info } => {
            fx.send(
                d.address,
                protocol::sensor_info_response(info.sensor_id, info.status),
            );
            if info.status <= 2 {
                register_sensor(d, info, at, fx);
                None
            } else {
                Some((15, "unknown sensor status"))
            }
        }
        Packet::Rotation {
            sensor_id,
            rotation,
            data_type,
            calibration,
            acceleration,
            fallback,
        } => {
            if let Some(s) = d.sensors.get_mut(&sensor_id) {
                recover_sensor(s, &d.key, at, fx);
            }
            if data_type != 1 {
                Some((
                    17,
                    "correction or unknown rotation type is not a pose sample",
                ))
            } else if let Some(s) = d.sensors.get_mut(&sensor_id) {
                let server = rotation.udp_to_server();
                s.rotation = Some(Timed {
                    value: server,
                    received_at_ms: at,
                });
                s.udp_rotation_timing = received_at_ms.map(|received_at_ms| ReceiveTiming {
                    received_at_ms,
                    processed_at_ms: at,
                });
                s.calibration = calibration;
                s.samples += 1;
                sample.sensor_id = sensor_id;
                sample.packet_rotation = Some(rotation);
                sample.server_rotation = Some(server);
                sample.compatibility_fallback = fallback;
                if let Some(a) = acceleration {
                    let server = if d.handshake.protocol_version >= 22 {
                        a
                    } else {
                        a.legacy_acceleration_to_server()
                    };
                    s.acceleration = Some(Timed {
                        value: server,
                        received_at_ms: at,
                    });
                    sample.packet_acceleration = Some(a);
                    sample.server_acceleration = Some(server);
                }
                counters.samples += 1;
                fx.event(at, EventKind::Sample { sample });
                None
            } else {
                Some((17, "sensor has not been announced"))
            }
        }
        Packet::Acceleration {
            sensor_id,
            acceleration,
        } => {
            if let Some(s) = d.sensors.get_mut(&sensor_id) {
                recover_sensor(s, &d.key, at, fx);
                let server = if d.handshake.protocol_version >= 22 {
                    acceleration
                } else {
                    acceleration.legacy_acceleration_to_server()
                };
                s.acceleration = Some(Timed {
                    value: server,
                    received_at_ms: at,
                });
                sample.sensor_id = sensor_id;
                sample.packet_acceleration = Some(acceleration);
                sample.server_acceleration = Some(server);
                counters.samples += 1;
                fx.event(at, EventKind::Sample { sample });
                None
            } else {
                Some((4, "sensor has not been announced"))
            }
        }
        Packet::Position {
            sensor_id,
            position,
        } => {
            if let Some(s) = d.sensors.get_mut(&sensor_id) {
                s.position = Some(Timed {
                    value: position,
                    received_at_ms: at,
                });
                sample.sensor_id = sensor_id;
                sample.position = Some(position);
                counters.samples += 1;
                fx.event(at, EventKind::Sample { sample });
                None
            } else {
                Some((27, "sensor has not been announced"))
            }
        }
        Packet::Ping { id } => {
            if let Some((expected, sent)) = d.pending_ping {
                if id == expected {
                    d.half_rtt_ms = Some(Timed {
                        value: at.saturating_sub(sent) / 2,
                        received_at_ms: at,
                    });
                    d.pending_ping = None;
                }
            }
            None
        }
        Packet::Features { flags } => {
            d.features = flags;
            d.features_available = true;
            fx.send(
                d.address,
                protocol::header(22, &[protocol::SERVER_FEATURES]),
            );
            None
        }
        Packet::Battery { voltage, fraction } => {
            if let Some(value) = voltage {
                d.battery_voltage = Some(Timed {
                    value,
                    received_at_ms: at,
                });
            }
            d.battery_fraction = Some(Timed {
                value: fraction,
                received_at_ms: at,
            });
            fx.event(
                at,
                EventKind::Telemetry {
                    device_key: d.key.clone(),
                    sensor_id: None,
                    name: "battery".into(),
                },
            );
            None
        }
        Packet::Signal { rssi, .. } => {
            d.rssi = Some(Timed {
                value: i16::from(rssi),
                received_at_ms: at,
            });
            fx.event(
                at,
                EventKind::Telemetry {
                    device_key: d.key.clone(),
                    sensor_id: None,
                    name: "rssi".into(),
                },
            );
            None
        }
        Packet::Temperature {
            sensor_id,
            temperature,
        } => {
            if let Some(s) = d.sensors.get_mut(&sensor_id) {
                s.temperature = Some(Timed {
                    value: temperature,
                    received_at_ms: at,
                });
                fx.event(
                    at,
                    EventKind::Telemetry {
                        device_key: d.key.clone(),
                        sensor_id: Some(sensor_id),
                        name: "temperature".into(),
                    },
                );
                None
            } else {
                Some((20, "sensor has not been announced"))
            }
        }
        Packet::Flex { sensor_id, value } => {
            if let Some(s) = d.sensors.get_mut(&sensor_id) {
                s.flex = Some(Timed {
                    value,
                    received_at_ms: at,
                });
                fx.event(
                    at,
                    EventKind::FlexValue {
                        device_key: d.key.clone(),
                        sensor_id,
                        session: d.session,
                        value,
                    },
                );
                None
            } else {
                Some((26, "sensor has not been announced"))
            }
        }
        Packet::Error { sensor_id, code } => {
            if let Some(s) = d.sensors.get_mut(&sensor_id) {
                s.error_code = Some(Timed {
                    value: code,
                    received_at_ms: at,
                });
                s.status = SensorStatus::Error;
                fx.event(
                    at,
                    EventKind::SensorState {
                        device_key: d.key.clone(),
                        sensor_id,
                        status: s.status,
                    },
                );
                None
            } else {
                Some((14, "sensor has not been announced"))
            }
        }
        Packet::Tap { sensor_id, tap } => {
            fx.event(
                at,
                EventKind::SensorTap {
                    device_key: d.key.clone(),
                    sensor_id,
                    tap,
                },
            );
            None
        }
        Packet::UserAction { action } => {
            fx.event(
                at,
                EventKind::UserAction {
                    device_key: d.key.clone(),
                    action,
                },
            );
            None
        }
        Packet::Serial { .. } => Some((11, "serial content retained only in recording")),
        Packet::ConfigAck {
            sensor_id,
            config_type,
        } => {
            d.ack_counter = d.ack_counter.saturating_add(1);
            if d.config_acks.len() >= 256 && !d.config_acks.contains_key(&(sensor_id, config_type))
            {
                d.config_acks.clear();
            }
            d.config_acks
                .insert((sensor_id, config_type), d.ack_counter);
            if let Some(state) = d.config_requests.remove(&(sensor_id, config_type)) {
                if config_type == 1 {
                    if let Some(sensor) = d.sensors.get_mut(&sensor_id) {
                        if let Some(flags) = sensor.info.config.as_mut() {
                            *flags = (*flags & !1) | u16::from(state);
                        }
                        fx.event(
                            at,
                            EventKind::SensorMetadata {
                                device_key: d.key.clone(),
                                sensor_id,
                                imu_type: sensor.info.imu_type,
                                data_type: sensor.info.data_type,
                                magnetometer_enabled: state,
                            },
                        );
                    }
                }
            }
            None
        }
        Packet::ProtocolChange { .. } => Some((
            200,
            "reserved protocol negotiation ignored as in the reference server",
        )),
        Packet::Unknown { packet_id, .. } => Some((packet_id, "unsupported packet")),
    };
    if let Some((id, reason)) = ignored {
        counters.ignored_packets += 1;
        fx.ignore(at, d.address, id, reason);
    }
}

fn recover_sensor(sensor: &mut SensorState, key: &str, at: u64, fx: &mut Effects) {
    if matches!(
        sensor.status,
        SensorStatus::TimedOut | SensorStatus::Disconnected
    ) {
        sensor.status = SensorStatus::Ok;
        fx.event(
            at,
            EventKind::SensorState {
                device_key: key.into(),
                sensor_id: sensor.info.sensor_id,
                status: SensorStatus::Ok,
            },
        );
    }
}
