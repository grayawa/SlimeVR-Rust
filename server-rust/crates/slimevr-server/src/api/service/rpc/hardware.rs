//! Serial, Wi-Fi provisioning and firmware requests.
use super::super::Service;
use crate::receiver::Receiver;
use solarxr_protocol::rpc;

impl Service {
    pub(super) fn rpc_hardware(
        &mut self,
        h: rpc::RpcMessageHeader<'_>,
        receiver: &mut Receiver,
        at: u64,
        tx: u32,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        match h.message_type() {
            rpc::RpcMessage::FirmwareUpdateRequest => {
                let r = h
                    .message_as_firmware_update_request()
                    .ok_or("missing firmware update")?;
                let serial = self.serial.as_ref().ok_or("serial worker is not running")?;
                let frame = self.firmware.queue(
                    r,
                    &self.config,
                    receiver,
                    &serial.ports,
                    &self.commands,
                    at,
                )?;
                if r.method_type() == rpc::FirmwareUpdateMethod::SerialFirmwareUpdate {
                    self.provisioning.stop(at);
                    self.provisioning_flush(receiver);
                }
                self.broadcast(frame);
            }
            rpc::RpcMessage::FirmwareUpdateStopQueuesRequest => {
                for frame in self.firmware.cancel(&self.commands) {
                    self.broadcast(frame);
                }
                self.provisioning.stop(at);
                self.provisioning_flush(receiver);
            }
            rpc::RpcMessage::StartWifiProvisioningRequest => {
                if self.firmware.serial_busy() {
                    return Err("serial port is reserved for firmware updating".into());
                }
                let r = h
                    .message_as_start_wifi_provisioning_request()
                    .ok_or("missing provisioning request")?;
                self.serial.as_ref().ok_or("serial worker is not running")?;
                self.provisioning.start(
                    r.ssid().ok_or("missing SSID")?.into(),
                    r.password().ok_or("missing password")?.into(),
                    r.port().map(str::to_owned),
                    at,
                )?;
                self.provisioning_flush(receiver);
                out.push(crate::serial::provisioning::frame(
                    tx,
                    self.provisioning.status,
                ));
            }
            rpc::RpcMessage::StopWifiProvisioningRequest => {
                self.provisioning.stop(at);
                self.provisioning_flush(receiver);
            }
            rpc::RpcMessage::SerialDevicesRequest => {
                out.push(crate::serial::list(
                    tx,
                    self.serial
                        .as_ref()
                        .map(|s| s.ports.as_slice())
                        .unwrap_or(&[]),
                ));
            }
            rpc::RpcMessage::OpenSerialRequest => {
                if self.firmware.serial_busy() {
                    return Err("serial port is reserved for firmware updating".into());
                }
                let r = h
                    .message_as_open_serial_request()
                    .ok_or("missing serial request")?;
                self.serial
                    .as_ref()
                    .ok_or("serial worker is not running")?
                    .send(crate::serial::Action::Open {
                        port: r.port().map(str::to_owned),
                        auto: r.auto(),
                        include_hid: true,
                    })?;
            }
            rpc::RpcMessage::CloseSerialRequest => {
                self.serial
                    .as_ref()
                    .ok_or("serial worker is not running")?
                    .send(crate::serial::Action::Close)?;
            }
            rpc::RpcMessage::SetWifiRequest => {
                let r = h
                    .message_as_set_wifi_request()
                    .ok_or("missing Wi-Fi request")?;
                let ssid = r.ssid().ok_or("missing SSID")?;
                let password = r.password().ok_or("missing password")?;
                crate::serial::wifi(ssid, password)?;
                self.serial
                    .as_ref()
                    .ok_or("serial worker is not running")?
                    .send(crate::serial::Action::Wifi {
                        ssid: ssid.into(),
                        password: password.into(),
                    })?;
            }
            rpc::RpcMessage::SerialTrackerRebootRequest
            | rpc::RpcMessage::SerialTrackerFactoryResetRequest
            | rpc::RpcMessage::SerialTrackerGetInfoRequest
            | rpc::RpcMessage::SerialTrackerGetWifiScanRequest
            | rpc::RpcMessage::SerialTrackerCustomCommandRequest => {
                let command = match h.message_type() {
                    rpc::RpcMessage::SerialTrackerRebootRequest => "REBOOT",
                    rpc::RpcMessage::SerialTrackerFactoryResetRequest => "FRST",
                    rpc::RpcMessage::SerialTrackerGetInfoRequest => "GET INFO",
                    rpc::RpcMessage::SerialTrackerGetWifiScanRequest => "GET WIFISCAN",
                    _ => h
                        .message_as_serial_tracker_custom_command_request()
                        .and_then(|r| r.command())
                        .ok_or("missing serial command")?,
                };
                crate::serial::command(command)?;
                self.serial
                    .as_ref()
                    .ok_or("serial worker is not running")?
                    .send(crate::serial::Action::Command(command.into()))?;
            }
            _ => unreachable!("RPC routed to the wrong domain"),
        }
        Ok(())
    }
}
