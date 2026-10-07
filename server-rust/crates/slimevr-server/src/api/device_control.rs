use super::protocol::rpc_frame;
use crate::receiver::{DeviceConfig, Effects, Receiver};
use solarxr_protocol::{datatypes as dt, rpc};
use std::collections::BTreeMap;
pub type Response = (u32, Option<(u8, u8)>, bool);
pub type Completion = (Option<Response>, Option<String>);
#[derive(Clone)]
struct Receipt {
    key: String,
    sensor: u8,
    session: u64,
    ack: u64,
}
struct Pending {
    started: u64,
    receipts: Vec<Receipt>,
    response: Option<Response>,
}
#[derive(Default)]
pub struct Controller {
    pending: Vec<Pending>,
    applied: BTreeMap<(String, u8, u64), bool>,
    blocked: BTreeMap<(String, u8, u64), u64>,
    pub outbound: Vec<(DeviceConfig, Effects)>,
}
impl Controller {
    pub fn ready(&self, commands: &[DeviceConfig]) -> Result<(), String> {
        if self.pending.iter().map(|p| p.receipts.len()).sum::<usize>() + commands.len() > 256 {
            return Err("device command queue is full".into());
        }
        if commands.iter().any(|c| {
            self.blocked
                .keys()
                .any(|(key, sensor, _)| *key == c.device_key && *sensor == c.sensor_id)
        }) {
            return Err("previous configuration is unconfirmed; wait for acknowledgement or reconnect the tracker".into());
        }
        if commands.iter().any(|c| {
            self.pending
                .iter()
                .flat_map(|p| &p.receipts)
                .any(|r| r.key == c.device_key && r.sensor == c.sensor_id)
        }) {
            return Err(
                "a configuration change for this tracker is still awaiting acknowledgement".into(),
            );
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        commands: Vec<DeviceConfig>,
        receiver: &mut Receiver,
        at: u64,
        response: Option<Response>,
    ) -> Result<(), String> {
        self.ready(&commands)?;
        for command in &commands {
            let d = receiver
                .devices
                .get(&command.device_key)
                .ok_or("unknown UDP device")?;
            if command.config_type != 1
                || d.transport_timed_out
                || !d.sensors.contains_key(&command.sensor_id)
            {
                return Err(
                    "magnetometer tracker is offline, unknown, or has an invalid config type"
                        .into(),
                );
            }
        }
        let mut receipts = Vec::new();
        for command in commands {
            let d = &receiver.devices[&command.device_key];
            let receipt = Receipt {
                key: command.device_key.clone(),
                sensor: command.sensor_id,
                session: d.session,
                ack: d
                    .config_acks
                    .get(&(command.sensor_id, 1))
                    .copied()
                    .unwrap_or(0),
            };
            let effects = receiver.config_command(&command)?;
            self.applied.insert(
                (
                    command.device_key.clone(),
                    command.sensor_id,
                    receipt.session,
                ),
                command.enabled,
            );
            self.outbound.push((command, effects));
            receipts.push(receipt);
        }
        self.pending.push(Pending {
            started: at,
            receipts,
            response,
        });
        Ok(())
    }
    pub fn tick(
        &mut self,
        receiver: &mut Receiver,
        config: &super::FrontendConfig,
        at: u64,
    ) -> Vec<Completion> {
        let mut completed = Vec::new();
        let mut next = Vec::new();
        for mut p in std::mem::take(&mut self.pending) {
            let mut disconnected = false;
            p.receipts.retain(|r| {
                let Some(d) = receiver.devices.get(&r.key) else {
                    disconnected = true;
                    return false;
                };
                if d.session != r.session || d.transport_timed_out {
                    disconnected = true;
                    return false;
                }
                d.config_acks.get(&(r.sensor, 1)).copied().unwrap_or(0) <= r.ack
            });
            if disconnected {
                completed.push((
                    p.response,
                    Some(
                        "tracker disconnected or restarted before confirming its configuration"
                            .into(),
                    ),
                ));
            } else if p.receipts.is_empty() {
                completed.push((p.response, None));
            } else if at.saturating_sub(p.started) >= 10000 {
                for r in &p.receipts {
                    self.blocked
                        .insert((r.key.clone(), r.sensor, r.session), r.ack);
                }
                completed.push((p.response,Some("tracker did not acknowledge the configuration within 10 seconds; desired settings were saved".into())));
            } else {
                next.push(p);
            }
        }
        self.pending = next;
        self.blocked.retain(|(key, sensor, session), ack| {
            receiver.devices.get(key).is_some_and(|d| {
                d.session == *session
                    && d.config_acks.get(&(*sensor, 1)).copied().unwrap_or(0) <= *ack
            })
        });
        self.applied.retain(|(key, sensor, session), _| {
            receiver
                .devices
                .get(key)
                .is_some_and(|d| d.session == *session && d.sensors.contains_key(sensor))
        });
        let mut commands = Vec::new();
        for d in receiver.devices.values().filter(|d| !d.transport_timed_out) {
            for (id, s) in &d.sensors {
                let Some(flags) = s.info.config else {
                    continue;
                };
                if flags & 2 == 0 {
                    continue;
                }
                let enabled = config.magnetometers_enabled
                    && config
                        .mag_preferences
                        .get(&format!("{}/{}", d.key, id))
                        .copied()
                        .unwrap_or(true);
                if !self.blocked.contains_key(&(d.key.clone(), *id, d.session))
                    && (flags & 1 != 0) != enabled
                    && self.applied.get(&(d.key.clone(), *id, d.session)) != Some(&enabled)
                    && !self
                        .pending
                        .iter()
                        .flat_map(|p| &p.receipts)
                        .any(|r| r.key == d.key && r.sensor == *id)
                {
                    commands.push(DeviceConfig {
                        device_key: d.key.clone(),
                        sensor_id: *id,
                        config_type: 1,
                        enabled,
                    });
                }
            }
        }
        if !commands.is_empty() {
            if let Err(e) = self.apply(commands, receiver, at, None) {
                completed.push((None, Some(e)));
            }
        }
        completed
    }
}
pub fn mag_frame(tx: u32, tracker: Option<(u8, u8)>, enabled: bool) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::MagToggleResponse, tx, |f| {
        let id = tracker.map(|(device, sensor)| {
            let device = dt::DeviceId::new(device);
            dt::TrackerId::create(
                f,
                &dt::TrackerIdArgs {
                    device_id: Some(&device),
                    tracker_num: sensor,
                },
            )
        });
        rpc::MagToggleResponse::create(
            f,
            &rpc::MagToggleResponseArgs {
                tracker_id: id,
                enable: enabled,
            },
        )
        .as_union_value()
    })
}
pub fn resolve(
    id: dt::TrackerId<'_>,
    config: &super::FrontendConfig,
    receiver: &Receiver,
) -> Result<(String, u8, u8), String> {
    let device = id.device_id().ok_or("missing device ID")?.id();
    let sensor = id.tracker_num();
    let key = config
        .device_ids
        .iter()
        .find(|(_, v)| **v == device)
        .map(|(k, _)| k.clone())
        .ok_or("unknown device ID")?;
    if !receiver
        .devices
        .get(&key)
        .is_some_and(|d| d.sensors.contains_key(&sensor))
    {
        return Err("unknown UDP sensor".into());
    }
    Ok((key, sensor, device))
}
