//! Compact device readings and immutable metadata, independent of receiver internals.
use crate::{
    protocol::Handshake,
    receiver::{DeviceState, Origin, SensorState},
};
use slimevr_core::Timed;
use std::{net::SocketAddr, ops::Deref, sync::Arc};

pub struct DeviceMetadata {
    pub key: String,
    pub session: u64,
    pub origin: Origin,
    pub address: SocketAddr,
    pub handshake: Handshake,
    pub display_name: Option<String>,
    pub firmware_date: Option<String>,
}
impl DeviceMetadata {
    pub(super) fn matches(&self, device: &DeviceState) -> bool {
        self.key == device.key
            && self.session == device.session
            && self.origin == device.origin
            && self.address == device.address
            && self.handshake == device.handshake
            && self.display_name == device.display_name
            && self.firmware_date == device.firmware_date
    }
    pub(super) fn new(device: &DeviceState) -> Self {
        Self {
            key: device.key.clone(),
            session: device.session,
            origin: device.origin,
            address: device.address,
            handshake: device.handshake.clone(),
            display_name: device.display_name.clone(),
            firmware_date: device.firmware_date.clone(),
        }
    }
}
#[derive(Clone)]
pub struct LiveDevice {
    pub metadata: Arc<DeviceMetadata>,
    pub sensors: Vec<(u8, SensorState)>,
    pub battery_runtime: Option<u64>,
    pub packet_loss: Option<f32>,
    pub packets_received: Option<i32>,
    pub packets_lost: Option<i32>,
    pub battery_voltage: Option<Timed<f32>>,
    pub battery_fraction: Option<Timed<f32>>,
    pub rssi: Option<Timed<i16>>,
    pub half_rtt_ms: Option<Timed<u64>>,
}
impl LiveDevice {
    pub(super) fn new(device: &DeviceState, metadata: Arc<DeviceMetadata>) -> Self {
        Self {
            metadata,
            sensors: device
                .sensors
                .iter()
                .map(|(id, sensor)| (*id, sensor.clone()))
                .collect(),
            battery_runtime: device.battery_runtime,
            packet_loss: device.packet_loss,
            packets_received: device.packets_received,
            packets_lost: device.packets_lost,
            battery_voltage: device.battery_voltage.clone(),
            battery_fraction: device.battery_fraction.clone(),
            rssi: device.rssi.clone(),
            half_rtt_ms: device.half_rtt_ms.clone(),
        }
    }
}
impl Deref for LiveDevice {
    type Target = DeviceMetadata;
    fn deref(&self) -> &DeviceMetadata {
        &self.metadata
    }
}
