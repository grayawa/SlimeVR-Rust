use super::*;
use crate::hid::Event as HidEvent;
impl Receiver {
    pub fn hid(&mut self, event: &HidEvent, at: u64) -> Effects {
        let mut fx = Effects::default();
        match event {
            HidEvent::Closed { path, session } => {
                if let Some(ids) = self.hid_sources.remove(&(path.clone(), *session)) {
                    for key in ids.into_values() {
                        if let Some(d) = self.devices.get_mut(&key) {
                            d.transport_timed_out = true;
                            fx.event(
                                at,
                                EventKind::TransportState {
                                    device_key: key.clone(),
                                    timed_out: true,
                                },
                            );
                            for s in d.sensors.values_mut() {
                                s.status = SensorStatus::Disconnected;
                                fx.event(
                                    at,
                                    EventKind::SensorState {
                                        device_key: key.clone(),
                                        sensor_id: s.info.sensor_id,
                                        status: s.status,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            HidEvent::Error { message } => fx.event(
                at,
                EventKind::Rejected {
                    address: "hid".into(),
                    reason: message.clone(),
                },
            ),
            HidEvent::Report {
                path,
                session,
                bytes,
            } => {
                if bytes.len() > 1024 || !bytes.len().is_multiple_of(16) || path.len() > 2048 {
                    fx.event(
                        at,
                        EventKind::Rejected {
                            address: "hid".into(),
                            reason: "invalid HID report size or path".into(),
                        },
                    );
                    return fx;
                }
                self.counters.received += 1;
                for data in bytes.as_chunks::<16>().0 {
                    if data.iter().all(|b| *b == 0) {
                        continue;
                    }
                    let id = data[1];
                    if data[0] == 255 {
                        let address =
                            u64::from_le_bytes(data[2..10].try_into().unwrap()) & 0xffffffffffff;
                        let key = format!("hid:{address:012X}");
                        let source = (path.clone(), *session);
                        if address == 0
                            || self.hid_sources.get(&source).and_then(|ids| ids.get(&id))
                                == Some(&key)
                        {
                            continue;
                        }
                        if !self.devices.contains_key(&key)
                            && self.devices.len() >= self.config.max_devices
                        {
                            continue;
                        }
                        // A new receiver session or a reassigned short ID must establish a new sample session.
                        if let Some(old) = self
                            .hid_sources
                            .get_mut(&source)
                            .and_then(|ids| ids.remove(&id))
                        {
                            if let Some(d) = self.devices.get_mut(&old) {
                                d.transport_timed_out = true;
                                fx.event(
                                    at,
                                    EventKind::TransportState {
                                        device_key: old,
                                        timed_out: true,
                                    },
                                );
                            }
                        }
                        for ids in self.hid_sources.values_mut() {
                            ids.retain(|_, v| v != &key);
                        }
                        let next = self
                            .devices
                            .get(&key)
                            .map_or(1, |d| d.session.saturating_add(1));
                        self.devices.insert(
                            key.clone(),
                            DeviceState {
                                display_name: None,
                                key: key.clone(),
                                origin: Origin::Hid,
                                address: "0.0.0.0:0".parse().unwrap(),
                                handshake: Handshake {
                                    board_type: 0,
                                    imu_type: 0,
                                    mcu_type: 0,
                                    protocol_version: 0,
                                    firmware: None,
                                    mac: None,
                                },
                                session: next,
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
                            },
                        );
                        self.hid_sources
                            .entry(source)
                            .or_default()
                            .insert(id, key.clone());
                        fx.event(
                            at,
                            EventKind::DeviceConnected {
                                device_key: key,
                                address: format!("hid:{path}"),
                                firmware: None,
                                session: next,
                                preserve_calibration: false,
                            },
                        );
                        continue;
                    }
                    let Some(key) = self
                        .hid_sources
                        .get(&(path.clone(), *session))
                        .and_then(|ids| ids.get(&id))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(d) = self.devices.get_mut(&key) else {
                        continue;
                    };
                    if d.transport_timed_out {
                        continue;
                    }
                    if data[0] == 0 {
                        d.handshake.board_type = u32::from(data[5]);
                        d.handshake.mcu_type = u32::from(data[6]);
                        d.handshake.imu_type = u32::from(data[8]);
                        d.handshake.firmware =
                            Some(format!("{}.{}.{}", data[12], data[13], data[14]));
                        let date = u16::from_le_bytes([data[10], data[11]]);
                        d.firmware_date = Some(format!(
                            "{:04}-{:02}-{:02}",
                            2020 + (date >> 9),
                            (date >> 5) & 15,
                            date & 31
                        ));
                        register_sensor(
                            d,
                            SensorInfo {
                                sensor_id: 0,
                                status: 1,
                                imu_type: data[8],
                                config: match data[9] {
                                    1 => Some(2),
                                    2 => Some(3),
                                    _ => None,
                                },
                                rest_calibrated: None,
                                body_position: None,
                                data_type: 0,
                            },
                            at,
                            &mut fx,
                        );
                    }
                    if !d.sensors.contains_key(&0) {
                        continue;
                    }
                    d.last_alive_ms = at;
                    d.last_sequence = d.last_sequence.saturating_add(1);
                    let s = d.sensors.get_mut(&0).unwrap();
                    s.last_alive_ms = at;
                    if s.status == SensorStatus::TimedOut {
                        s.status = SensorStatus::Ok;
                        d.sleep_at = None;
                        fx.event(
                            at,
                            EventKind::SensorState {
                                device_key: key.clone(),
                                sensor_id: 0,
                                status: s.status,
                            },
                        );
                    }
                    if matches!(data[0], 0 | 2) {
                        d.battery_fraction = (data[2] != 128).then_some(Timed {
                            value: f32::from(data[2] & 127) / 100.0,
                            received_at_ms: at,
                        });
                        d.battery_voltage = Some(Timed {
                            value: (f32::from(data[3]) + 245.0) / 100.0,
                            received_at_ms: at,
                        });
                        s.temperature = (data[4] != 0).then_some(Timed {
                            value: f32::from(data[4]) / 2.0 - 39.0,
                            received_at_ms: at,
                        });
                    }
                    if matches!(data[0], 0 | 2 | 3 | 6 | 7) {
                        d.rssi = Some(Timed {
                            value: -i16::from(data[15]),
                            received_at_ms: at,
                        });
                    }
                    if data[0] == 3 {
                        let status = match data[2] {
                            0 => Some(SensorStatus::Disconnected),
                            1 => Some(SensorStatus::Ok),
                            2 => Some(SensorStatus::Busy),
                            3 => Some(SensorStatus::Error),
                            4 => Some(SensorStatus::Occluded),
                            5 => Some(SensorStatus::TimedOut),
                            _ => None,
                        };
                        if let Some(status) = status {
                            s.status = status;
                            fx.event(
                                at,
                                EventKind::SensorState {
                                    device_key: key.clone(),
                                    sensor_id: 0,
                                    status,
                                },
                            );
                        }
                        d.packets_received = Some(i32::from(data[4]));
                        d.packets_lost = Some(i32::from(data[5]));
                        d.packet_loss = Some(if data[5] == 0 {
                            0.0
                        } else {
                            f32::from(data[5]) / (f32::from(data[4]) + f32::from(data[5]))
                        });
                    }
                    if data[0] == 5 {
                        let runtime = u64::from_le_bytes(data[2..10].try_into().unwrap());
                        if runtime <= i64::MAX as u64 {
                            d.battery_runtime = Some(runtime);
                        }
                    }
                    if matches!(data[0], 6 | 7) {
                        let changed = data[2] & !s.button;
                        if changed & 3 != 0 && data[2] & 3 != 0 {
                            fx.event(
                                at,
                                EventKind::TapSetup {
                                    device_key: key.clone(),
                                    sensor_id: 0,
                                },
                            );
                        }
                        s.button = data[2];
                        let timeout = u16::from_be_bytes([data[3], data[4]]);
                        if timeout != 0 {
                            d.sleep_at = (timeout != u16::MAX).then_some(at + u64::from(timeout));
                        }
                    }
                    if let Some(rotation) = crate::hid::rotation(data) {
                        let acceleration = match data[0] {
                            1 => Some(crate::hid::vector(data, 10, 1.0 / 128.0)),
                            2 | 7 => Some(crate::hid::vector(data, 9, 1.0 / 128.0)),
                            _ => None,
                        };
                        s.rotation = Some(Timed {
                            value: rotation,
                            received_at_ms: at,
                        });
                        if let Some(value) = acceleration {
                            s.acceleration = Some(Timed {
                                value,
                                received_at_ms: at,
                            });
                        }
                        if data[0] == 4 {
                            s.magnetic_vector = Some(Timed {
                                value: crate::hid::vector(data, 10, 1000.0 / 1024.0),
                                received_at_ms: at,
                            });
                        }
                        s.samples += 1;
                        self.counters.samples += 1;
                        fx.event(
                            at,
                            EventKind::Sample {
                                sample: TrackerSample {
                                    source: "hid".into(),
                                    device_key: key,
                                    sensor_id: 0,
                                    session: d.session,
                                    packet_sequence: d.last_sequence,
                                    received_at_ms: at,
                                    socket_received_at_ms: None,
                                    sensor_timestamp_us: None,
                                    packet_rotation: None,
                                    server_rotation: Some(rotation),
                                    packet_acceleration: acceleration,
                                    server_acceleration: acceleration,
                                    position: None,
                                    compatibility_fallback: false,
                                },
                            },
                        );
                    }
                }
                self.counters.accepted += 1;
            }
        }
        fx
    }
}
