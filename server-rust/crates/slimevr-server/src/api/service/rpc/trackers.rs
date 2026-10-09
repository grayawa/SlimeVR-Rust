//! Tracker assignment, admission and magnetometer requests.
use super::super::Service;
use crate::{
    api::{device_control, protocol},
    receiver::{normalize_mac, Receiver},
};
use slimevr_core::{
    pose::{PoseEngine, TrackerBinding},
    Quaternion as Q,
};
use solarxr_protocol::rpc;

impl Service {
    pub(super) fn rpc_trackers(
        &mut self,
        h: rpc::RpcMessageHeader<'_>,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
        at: u64,
        tx: u32,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        match h.message_type() {
            rpc::RpcMessage::MagToggleRequest => {
                let r = h
                    .message_as_mag_toggle_request()
                    .ok_or("missing magnetometer query")?;
                let (tracker, enabled) = if let Some(id) = r.tracker_id() {
                    let (key, sensor, device) =
                        device_control::resolve(id, &self.config, receiver)?;
                    (
                        Some((device, sensor)),
                        self.config
                            .mag_preferences
                            .get(&format!("{key}/{sensor}"))
                            .copied()
                            .unwrap_or(true),
                    )
                } else {
                    (None, self.config.magnetometers_enabled)
                };
                out.push(device_control::mag_frame(tx, tracker, enabled));
            }
            rpc::RpcMessage::ChangeMagToggleRequest => {
                let r = h
                    .message_as_change_mag_toggle_request()
                    .ok_or("missing magnetometer change")?;
                let mut c = self.config.clone();
                let mut commands = Vec::new();
                let tracker = if let Some(id) = r.tracker_id() {
                    let (key, sensor, device) = device_control::resolve(id, &c, receiver)?;
                    c.mag_preferences
                        .insert(format!("{key}/{sensor}"), r.enable());
                    if c.magnetometers_enabled {
                        let d = &receiver.devices[&key];
                        if d.sensors[&sensor].info.config.is_none_or(|v| v & 2 == 0) {
                            return Err(
                                "this sensor does not support magnetometer configuration".into()
                            );
                        }
                        commands.push(crate::receiver::DeviceConfig {
                            device_key: key,
                            sensor_id: sensor,
                            config_type: 1,
                            enabled: r.enable(),
                        });
                    }
                    Some((device, sensor))
                } else {
                    c.magnetometers_enabled = r.enable();
                    for d in receiver.devices.values().filter(|d| !d.transport_timed_out) {
                        for (sensor, s) in &d.sensors {
                            if s.info.config.is_some_and(|v| v & 2 != 0) {
                                commands.push(crate::receiver::DeviceConfig {
                                    device_key: d.key.clone(),
                                    sensor_id: *sensor,
                                    config_type: 1,
                                    enabled: c.magnetometers_enabled
                                        && c.mag_preferences
                                            .get(&format!("{}/{}", d.key, sensor))
                                            .copied()
                                            .unwrap_or(true),
                                });
                            }
                        }
                    }
                    None
                };
                self.device_control.ready(&commands)?;
                self.install_config(c)?;
                if commands.is_empty() {
                    out.push(device_control::mag_frame(tx, tracker, r.enable()));
                } else {
                    self.device_control.apply(
                        commands,
                        receiver,
                        at,
                        Some((tx, tracker, r.enable())),
                    )?;
                }
            }
            rpc::RpcMessage::AssignTrackerRequest => {
                let r = h
                    .message_as_assign_tracker_request()
                    .ok_or("missing assignment")?;
                let id = r.tracker_id().ok_or("missing tracker id")?;
                let device = id.device_id().ok_or("cannot assign computed tracker")?.id();
                let key = self
                    .config
                    .device_ids
                    .iter()
                    .find(|(_, v)| **v == device)
                    .map(|(k, _)| k.clone())
                    .ok_or("unknown device")?;
                let sensor = id.tracker_num();
                if !receiver
                    .devices
                    .get(&key)
                    .is_some_and(|d| d.sensors.contains_key(&sensor))
                {
                    return Err("unknown sensor".into());
                }
                let mut c = self.config.clone();
                let previous = c
                    .pose
                    .bindings
                    .iter()
                    .find(|b| b.device_key == key && b.sensor_id == sensor)
                    .cloned();
                c.pose
                    .bindings
                    .retain(|b| b.device_key != key || b.sensor_id != sensor);
                if r.body_position() != solarxr_protocol::datatypes::BodyPart::NONE {
                    let body = protocol::from_body(r.body_position())?;
                    c.pose.bindings.retain(|binding| binding.body != body);
                    let mounting = r
                        .mounting_orientation()
                        .map(protocol::from_quat)
                        .or_else(|| previous.as_ref().map(|b| b.mounting))
                        .unwrap_or_else(|| Q::rotation_y(std::f32::consts::PI));
                    c.pose.bindings.push(TrackerBinding {
                        device_key: key.clone(),
                        sensor_id: sensor,
                        body,
                        mounting,
                    });
                }
                if let Some(name) = r.display_name() {
                    if name.len() > 128 {
                        return Err("tracker name too long".into());
                    }
                    c.tracker_names
                        .insert(format!("{key}/{sensor}"), name.into());
                }
                if r.mounting_orientation().is_some()
                    && receiver.devices[&key].sensors[&sensor]
                        .capabilities
                        .allow_mounting
                {
                    if let Some(binding) = &previous {
                        c.pose.saved_mounting_resets.remove(&binding.body);
                    }
                    crate::config::put(
                        &mut c.yaml,
                        &["resetsConfig", "lastMountingMethod"],
                        "MANUAL",
                    )
                    .map_err(|e| e.to_string())?;
                }
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::ForgetDeviceRequest => {
                let r = h
                    .message_as_forget_device_request()
                    .ok_or("missing device")?;
                let key = normalize_mac(r.mac_address().ok_or("missing MAC")?)?;
                let mut c = self.config.clone();
                c.allowed_macs.retain(|m| m != &key);
                c.pose.bindings.retain(|b| b.device_key != key);
                c.tracker_names
                    .retain(|k, _| !k.starts_with(&format!("{key}/")));
                c.mag_preferences
                    .retain(|k, _| !k.starts_with(&format!("{key}/")));
                c.device_ids.remove(&key);
                crate::config::forget_device(&mut c, &key);
                self.commit(c, engine, receiver, at)?;
                receiver.config.allowed_macs = self.config.allowed_macs.clone();
                receiver.forget_device(&key);
                self.forgotten_devices.push(key);
            }
            rpc::RpcMessage::AddUnknownDeviceRequest => {
                let r = h
                    .message_as_add_unknown_device_request()
                    .ok_or("missing device approval")?;
                let mac = normalize_mac(r.mac_address().ok_or("missing MAC")?)?;
                let mut c = self.config.clone();
                if !c.allowed_macs.contains(&mac) {
                    c.allowed_macs.push(mac);
                }
                self.install_config(c)?;
                receiver.config.allowed_macs = self.config.allowed_macs.clone();
            }
            _ => unreachable!("RPC routed to the wrong domain"),
        }
        Ok(())
    }
}
