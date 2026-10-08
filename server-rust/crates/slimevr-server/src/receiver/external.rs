//! Positional input devices retain their own reset/filter capabilities and never enter UDP maintenance.
use super::*;
impl Receiver {
    pub fn external(&mut self, event: &InputEvent) -> Result<bool, String> {
        let at = event.at_ms;
        match &event.kind {
            EventKind::ExternalTracker {
                device_key,
                sensor_id,
                source,
                name,
                capabilities,
                ..
            } => {
                let origin = match source.as_str() {
                    "steamvr" => Origin::SteamVr,
                    "osc" => Origin::Osc,
                    "vrchat" => Origin::Vrchat,
                    "vmc" => Origin::Vmc,
                    _ => return Err("unknown external input source".into()),
                };
                if device_key.len() > 128
                    || device_key.is_empty()
                    || name.len() > 256
                    || !device_key.starts_with(&format!("{source}:"))
                {
                    return Err("invalid external tracker identity".into());
                }
                if !self.devices.contains_key(device_key)
                    && self.devices.len() >= self.config.max_devices
                {
                    return Ok(false);
                }
                let d = self
                    .devices
                    .entry(device_key.clone())
                    .or_insert_with(|| DeviceState {
                        key: device_key.clone(),
                        display_name: Some(name.clone()),
                        origin,
                        address: "0.0.0.0:0".parse().unwrap(),
                        handshake: Handshake {
                            board_type: 0,
                            imu_type: 0,
                            mcu_type: 0,
                            protocol_version: 0,
                            firmware: None,
                            mac: None,
                        },
                        session: 1,
                        config_acks: Default::default(),
                        config_requests: Default::default(),
                        ack_counter: 0,
                        last_alive_ms: at,
                        last_sequence: 0,
                        transport_timed_out: false,
                        sensors: Default::default(),
                        features: vec![],
                        features_available: true,
                        battery_voltage: None,
                        battery_fraction: None,
                        rssi: None,
                        half_rtt_ms: None,
                        pending_ping: None,
                        sleep_at: None,
                        battery_runtime: None,
                        firmware_date: None,
                        packet_loss: None,
                        packets_received: None,
                        packets_lost: None,
                    });
                if d.origin != origin {
                    return Err("external source identity collision".into());
                }
                d.display_name = Some(name.clone());
                d.last_alive_ms = at;
                d.transport_timed_out = false;
                d.sensors.entry(*sensor_id).or_insert_with(|| SensorState {
                    info: SensorInfo {
                        sensor_id: *sensor_id,
                        status: 1,
                        imu_type: 0,
                        config: None,
                        rest_calibrated: None,
                        body_position: None,
                        data_type: 0,
                    },
                    capabilities: *capabilities,
                    status: SensorStatus::Ok,
                    last_alive_ms: at,
                    rotation: None,
                    udp_rotation_timing: None,
                    acceleration: None,
                    position: None,
                    temperature: None,
                    flex: None,
                    calibration: None,
                    error_code: None,
                    samples: 0,
                    magnetic_vector: None,
                    button: 0,
                });
                if let Some(s) = d.sensors.get_mut(sensor_id) {
                    s.capabilities = *capabilities;
                }
                Ok(true)
            }
            EventKind::DeviceConnected {
                device_key,
                session,
                ..
            } => {
                let Some(d) = self.devices.get_mut(device_key).filter(|d| {
                    matches!(
                        d.origin,
                        Origin::SteamVr | Origin::Osc | Origin::Vrchat | Origin::Vmc
                    )
                }) else {
                    return Ok(false);
                };
                d.session = *session;
                d.last_alive_ms = at;
                d.transport_timed_out = false;
                for s in d.sensors.values_mut() {
                    s.status = SensorStatus::Ok;
                    s.rotation = None;
                    s.position = None;
                    s.last_alive_ms = at;
                }
                Ok(true)
            }
            EventKind::Sample { sample } => {
                let Some(d) = self.devices.get_mut(&sample.device_key).filter(|d| {
                    matches!(
                        d.origin,
                        Origin::SteamVr | Origin::Osc | Origin::Vrchat | Origin::Vmc
                    )
                }) else {
                    return Ok(false);
                };
                let Some(s) = d.sensors.get_mut(&sample.sensor_id) else {
                    return Ok(false);
                };
                if !sample
                    .device_key
                    .starts_with(&format!("{}:", sample.source))
                {
                    return Err("external source mismatch".into());
                }
                if sample.server_rotation.is_some_and(|q| !q.is_rotation())
                    || sample.position.is_some_and(|v| !v.is_finite())
                {
                    return Err("invalid external pose".into());
                }
                if d.session != sample.session {
                    return Ok(false);
                }
                d.last_alive_ms = at;
                d.last_sequence = sample.packet_sequence;
                d.transport_timed_out = false;
                s.last_alive_ms = at;
                if matches!(
                    s.status,
                    SensorStatus::Disconnected | SensorStatus::TimedOut
                ) {
                    s.status = SensorStatus::Ok;
                }
                s.position = sample.position.map(|value| Timed {
                    value,
                    received_at_ms: at,
                });
                if let Some(value) = sample.server_rotation {
                    s.rotation = Some(Timed {
                        value,
                        received_at_ms: at,
                    });
                }
                if let Some(value) = sample.position {
                    s.position = Some(Timed {
                        value,
                        received_at_ms: at,
                    });
                }
                s.samples += 1;
                Ok(true)
            }
            EventKind::SensorState {
                device_key,
                sensor_id,
                status,
            } => {
                let Some(d) = self.devices.get_mut(device_key).filter(|d| {
                    matches!(
                        d.origin,
                        Origin::SteamVr | Origin::Osc | Origin::Vrchat | Origin::Vmc
                    )
                }) else {
                    return Ok(false);
                };
                if let Some(s) = d.sensors.get_mut(sensor_id) {
                    s.status = *status;
                    if !matches!(status, SensorStatus::Ok | SensorStatus::Busy) {
                        s.position = None;
                    }
                    s.last_alive_ms = at;
                }
                if *status == SensorStatus::Disconnected {
                    d.transport_timed_out = true;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
