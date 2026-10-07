//! Serial, provisioning, firmware and device event coordination.
use super::Service;
use crate::{
    api::{device_control, protocol::rpc_frame, Wire},
    receiver::Receiver,
};
use solarxr_protocol::rpc;

impl Service {
    pub fn serial_event(&mut self, event: crate::serial::Event, receiver: &mut Receiver, at: u64) {
        if let Some((port, ssid, password)) = self.firmware.serial_event(&event, at) {
            if let Err(e) = self.provisioning.start(ssid, password, Some(port), at) {
                self.error(e);
            }
        }
        self.provisioning.event(&event, at);
        use crate::serial::Event;
        match event {
            Event::Suspended(token) => {
                if let Some(serial) = &mut self.serial {
                    serial.current = None;
                }
                self.firmware.suspended(token, &self.commands);
            }
            Event::Ports(ports) => {
                if let Some(serial) = &mut self.serial {
                    let added = ports
                        .iter()
                        .filter(|p| !serial.ports.contains(p))
                        .cloned()
                        .collect::<Vec<_>>();
                    serial.ports = ports;
                    for port in added {
                        self.broadcast(rpc_frame(
                            rpc::RpcMessage::NewSerialDeviceResponse,
                            0,
                            |f| {
                                let device = crate::serial::device(f, &port);
                                rpc::NewSerialDeviceResponse::create(
                                    f,
                                    &rpc::NewSerialDeviceResponseArgs {
                                        device: Some(device),
                                    },
                                )
                                .as_union_value()
                            },
                        ));
                    }
                }
            }
            Event::Connected(port) => {
                if let Some(serial) = &mut self.serial {
                    serial.current = Some(port.clone());
                }
                let _ = self.events.send(Wire::Serial(crate::serial::update(
                    Some(&port),
                    None,
                    false,
                )));
            }
            Event::Closed => {
                if let Some(serial) = &mut self.serial {
                    serial.current = None;
                }
                let _ = self
                    .events
                    .send(Wire::Serial(crate::serial::update(None, None, true)));
            }
            Event::Log { log, .. } => {
                let port = self.serial.as_ref().and_then(|s| s.current.as_ref());
                let _ =
                    self.events
                        .send(Wire::Serial(crate::serial::update(port, Some(&log), false)));
            }
            Event::Error(error) => self.error(error),
        }
        self.provisioning_flush(receiver);
    }
    pub(super) fn provisioning_flush(&mut self, receiver: &mut Receiver) {
        let (actions, statuses, macs) = self.provisioning.drain();
        for action in actions {
            if let Some(serial) = &self.serial {
                if let Err(e) = serial.send(action) {
                    self.error(e);
                }
            }
        }
        for status in statuses {
            let port = self
                .serial
                .as_ref()
                .and_then(|s| s.current.as_ref())
                .map(|p| p.path.as_str());
            if let Some(frame) = self
                .firmware
                .provisioning(status, port, self.provisioning.now())
            {
                self.broadcast(frame);
            }
            let _ = self
                .events
                .send(Wire::Provisioning(crate::serial::provisioning::frame(
                    0, status,
                )));
        }
        let mut changed = false;
        for mac in macs {
            if !self.config.allowed_macs.contains(&mac) {
                self.config.allowed_macs.push(mac);
                changed = true;
            }
        }
        if changed {
            receiver.config.allowed_macs = self.config.allowed_macs.clone();
            if let Err(e) = self.config.save(self.state_path.as_deref()) {
                self.error(e);
            }
        }
    }
    pub fn firmware_event(
        &mut self,
        event: crate::firmware::Event,
        receiver: &mut Receiver,
        at: u64,
    ) {
        let Some(serial) = &self.serial else {
            return;
        };
        let key = match &event {
            crate::firmware::Event::Failed { key, .. } => Some(key.clone()),
            _ => match &event {
                crate::firmware::Event::Ready { key, .. } => Some(key.clone()),
                _ => None,
            },
        };
        match self.firmware.event(event, serial, &self.commands, at) {
            Ok((frame, provision)) => {
                if let Some(frame) = frame {
                    self.broadcast(frame);
                }
                if let Some((port, ssid, password)) = provision {
                    if let Err(e) = self.provisioning.start(ssid, password, Some(port), at) {
                        self.error(e);
                    }
                }
            }
            Err(error) => {
                if let Some(key) = key {
                    if let Some(frame) = self.firmware.status_frame(&key) {
                        self.broadcast(frame);
                    }
                }
                self.error(error);
            }
        }
        self.provisioning_flush(receiver);
    }
    pub fn device_tick(&mut self, receiver: &mut Receiver, at: u64) {
        for frame in self.firmware.tick(receiver, &self.commands, at) {
            self.broadcast(frame);
        }
        let ports = self
            .serial
            .as_ref()
            .map(|s| s.ports.as_slice())
            .unwrap_or(&[]);
        self.provisioning.tick(at, ports);
        self.provisioning_flush(receiver);
        for (response, error) in self.device_control.tick(receiver, &self.config, at) {
            if let Some(error) = error {
                self.error(error);
            }
            if let Some((tx, tracker, enabled)) = response {
                self.broadcast(device_control::mag_frame(tx, tracker, enabled));
            }
        }
    }
}
