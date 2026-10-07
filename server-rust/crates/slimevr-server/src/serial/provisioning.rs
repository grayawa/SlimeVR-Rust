//! Firmware-compatible Wi-Fi provisioning, driven by the server's monotonic clock.
use super::{Action, Event, Port};
use crate::api::protocol::rpc_frame;
use solarxr_protocol::rpc::{self, WifiProvisioningStatus as S};
#[derive(Default)]
pub struct Provisioner {
    pub running: bool,
    pub status: S,
    ssid: String,
    password: String,
    port: Option<String>,
    since: u64,
    retry_at: u64,
    started: u64,
    has_logs: bool,
    actions: Vec<Action>,
    statuses: Vec<S>,
    pub admitted: Vec<String>,
}
impl Provisioner {
    pub fn now(&self) -> u64 {
        self.since
    }

    pub fn start(
        &mut self,
        ssid: String,
        password: String,
        port: Option<String>,
        at: u64,
    ) -> Result<(), String> {
        super::wifi(&ssid, &password)?;
        *self = Self {
            running: true,
            ssid,
            password,
            port,
            started: at,
            since: at,
            ..Default::default()
        };
        self.set(S::SERIAL_INIT, at);
        self.open();
        Ok(())
    }
    pub fn stop(&mut self, at: u64) {
        self.running = false;
        self.ssid.clear();
        self.password.clear();
        self.actions.clear();
        self.actions.push(Action::Close);
        self.set(S::NONE, at);
    }
    fn set(&mut self, status: S, at: u64) {
        if self.status != status {
            self.status = status;
            self.since = at;
            self.statuses.push(status);
        }
    }
    fn open(&mut self) {
        self.actions.push(Action::Open {
            port: self.port.clone(),
            auto: self.port.is_none(),
            include_hid: false,
        });
    }
    pub fn event(&mut self, event: &Event, at: u64) {
        if !self.running {
            return;
        }
        match event {
            Event::Ports(ports) if self.status == S::SERIAL_INIT && self.available(ports) => {
                self.open()
            }
            Event::Connected(_) => {
                self.has_logs = false;
                self.set(S::OBTAINING_MAC_ADDRESS, at);
                self.retry_at = at + 1000;
                self.actions.push(Action::Command("GET INFO".into()));
            }
            Event::Closed if self.status != S::DONE => {
                self.set(S::SERIAL_INIT, at);
                self.retry_at = at + 3000;
            }
            Event::Log { log, server: false } => {
                self.has_logs = true;
                if self.status == S::NO_SERIAL_LOGS_ERROR {
                    self.set(S::OBTAINING_MAC_ADDRESS, at);
                    self.actions.push(Action::Command("GET INFO".into()));
                }
                if self.status == S::OBTAINING_MAC_ADDRESS {
                    if let Some(mac) = mac(log) {
                        self.admitted.push(mac);
                        self.actions.push(Action::Wifi {
                            ssid: self.ssid.clone(),
                            password: self.password.clone(),
                        });
                        self.set(S::PROVISIONING, at);
                    }
                }
                if log.contains("New wifi credentials set") {
                    self.set(S::CONNECTING, at);
                }
                if log.contains("Looking for the server")
                    || log.contains("Searching for the server")
                {
                    self.set(S::LOOKING_FOR_SERVER, at);
                }
                if log.contains("Handshake successful") {
                    self.set(S::DONE, at);
                    self.password.clear();
                }
                if log.contains("Can't connect from any credentials") {
                    self.set(S::CONNECTION_ERROR, at);
                }
            }
            _ => {}
        }
    }
    fn available(&self, ports: &[Port]) -> bool {
        ports.iter().any(|p| {
            self.port
                .as_ref()
                .map_or(p.kind() == rpc::SerialDeviceType::ESP_TRACKER, |port| {
                    *port == p.path
                })
        })
    }
    pub fn tick(&mut self, at: u64, ports: &[Port]) {
        if !self.running {
            return;
        }
        let elapsed = at.saturating_sub(self.since);
        match self.status {
            S::SERIAL_INIT => {
                if !self.available(ports) && at.saturating_sub(self.started) >= 15000 {
                    self.set(S::NO_SERIAL_DEVICE_FOUND, at);
                } else if at >= self.retry_at {
                    self.retry_at = at + 3000;
                    if self.available(ports) {
                        self.open();
                    }
                }
            }
            S::OBTAINING_MAC_ADDRESS => {
                if !self.has_logs && elapsed >= 1000 {
                    self.set(S::NO_SERIAL_LOGS_ERROR, at);
                } else if elapsed >= 10000 {
                    self.set(S::CONNECTION_ERROR, at);
                } else if at >= self.retry_at {
                    self.retry_at = at + 1000;
                    self.actions.push(Action::Command("GET INFO".into()));
                }
            }
            S::PROVISIONING if elapsed >= 10000 => self.set(S::CONNECTION_ERROR, at),
            S::CONNECTING if elapsed >= 30000 => self.set(S::CONNECTION_ERROR, at),
            S::LOOKING_FOR_SERVER if elapsed >= 10000 => self.set(S::COULD_NOT_FIND_SERVER, at),
            _ => {}
        }
    }
    pub fn drain(&mut self) -> (Vec<Action>, Vec<S>, Vec<String>) {
        (
            std::mem::take(&mut self.actions),
            std::mem::take(&mut self.statuses),
            std::mem::take(&mut self.admitted),
        )
    }
}
pub fn mac(log: &str) -> Option<String> {
    let offset = log.to_ascii_lowercase().find("mac: ")? + 5;
    let value = log.get(offset..offset + 17)?;
    crate::receiver::normalize_mac(value).ok()
}
pub fn frame(tx: u32, status: S) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::WifiProvisioningStatusResponse, tx, |f| {
        rpc::WifiProvisioningStatusResponse::create(
            f,
            &rpc::WifiProvisioningStatusResponseArgs { status },
        )
        .as_union_value()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firmware_success_and_timeouts() {
        let port = Port {
            path: "COM8".into(),
            name: "ESP".into(),
            vid: 0x303a,
            pid: 0x1001,
        };
        let mut p = Provisioner::default();
        p.start("ssid".into(), "secret".into(), Some("COM8".into()), 0)
            .unwrap();
        p.drain();
        p.event(&Event::Connected(port.clone()), 10);
        p.event(
            &Event::Log {
                log: "mac: aa:bb:cc:dd:ee:ff, imu: 1".into(),
                server: false,
            },
            20,
        );
        let (actions, _, admitted) = p.drain();
        assert_eq!(admitted, ["AA:BB:CC:DD:EE:FF"]);
        assert!(actions.iter().any(|a| matches!(a, Action::Wifi { .. })));
        for (at, text, status) in [
            (30, "New wifi credentials set", S::CONNECTING),
            (40, "Looking for the server", S::LOOKING_FOR_SERVER),
            (50, "Handshake successful", S::DONE),
        ] {
            p.event(
                &Event::Log {
                    log: text.into(),
                    server: false,
                },
                at,
            );
            assert_eq!(p.status, status);
        }
        assert!(p.password.is_empty());
        p.tick(99999, std::slice::from_ref(&port));
        assert_eq!(p.status, S::DONE);
        p.start("ssid".into(), "secret".into(), None, 100000)
            .unwrap();
        p.event(&Event::Connected(port.clone()), 100001);
        p.tick(101002, std::slice::from_ref(&port));
        assert_eq!(p.status, S::NO_SERIAL_LOGS_ERROR);
        p.event(
            &Event::Log {
                log: "boot".into(),
                server: false,
            },
            101003,
        );
        assert_eq!(p.status, S::OBTAINING_MAC_ADDRESS);
        p.event(
            &Event::Log {
                log: "Searching for the server".into(),
                server: false,
            },
            101004,
        );
        p.tick(111004, &[port]);
        assert_eq!(p.status, S::COULD_NOT_FIND_SERVER);
        p.stop(111005);
        assert!(!p.running);
        assert!(p.password.is_empty());
        assert_eq!(p.status, S::NONE);
    }
    #[test]
    fn missing_device_and_injected_credentials() {
        let mut p = Provisioner::default();
        assert!(p.start("bad\n".into(), "".into(), None, 0).is_err());
        p.start("ssid".into(), "".into(), None, 0).unwrap();
        p.tick(15000, &[]);
        assert_eq!(p.status, S::NO_SERIAL_DEVICE_FOUND);
        assert_eq!(
            mac("mac: aa-bb-cc-dd-ee-ff, test"),
            Some("AA:BB:CC:DD:EE:FF".into())
        );
    }
}
