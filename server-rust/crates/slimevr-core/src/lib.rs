//! Deterministic input and pose algorithms, without sockets or a desktop host.
pub mod calibration;
pub mod filtering;
pub mod math;
pub mod pose;
pub mod skeleton;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Quaternion {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Only the UDP coordinate adaptation; no sensor/body mounting calibration.
    pub fn udp_to_server(self) -> Self {
        let c = std::f32::consts::FRAC_1_SQRT_2;
        Self {
            w: c * (self.w + self.x),
            x: c * (self.x - self.w),
            y: c * (self.y + self.z),
            z: c * (self.z - self.y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    pub fn legacy_acceleration_to_server(self) -> Self {
        // SENSOR_OFFSET_CORRECTION: -pi/2 around Z, used only below protocol 22.
        Self {
            x: self.y,
            y: -self.x,
            z: self.z,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timed<T> {
    pub value: T,
    /// Milliseconds on the receiver's monotonic clock, not an IMU sample time.
    pub received_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorStatus {
    Ok,
    Busy,
    Occluded,
    Error,
    TimedOut,
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackerSample {
    pub source: String,
    pub device_key: String,
    pub sensor_id: u8,
    pub session: u64,
    pub packet_sequence: i64,
    pub received_at_ms: u64,
    pub sensor_timestamp_us: Option<u64>,
    /// Device-fused quaternion, exactly after wire decoding and before AXES_OFFSET.
    pub packet_rotation: Option<Quaternion>,
    pub server_rotation: Option<Quaternion>,
    pub packet_acceleration: Option<Vector3>,
    pub server_acceleration: Option<Vector3>,
    pub position: Option<Vector3>,
    pub compatibility_fallback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerCapabilities {
    pub allow_reset: bool,
    pub allow_mounting: bool,
    pub allow_filter: bool,
    pub is_imu: bool,
}
impl Default for TrackerCapabilities {
    fn default() -> Self {
        Self {
            allow_reset: true,
            allow_mounting: true,
            allow_filter: true,
            is_imu: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputEvent {
    pub at_ms: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    ExternalTracker {
        device_key: String,
        sensor_id: u8,
        source: String,
        name: String,
        body: Option<skeleton::BodyPosition>,
        capabilities: TrackerCapabilities,
    },
    TapSetup {
        device_key: String,
        sensor_id: u8,
    },
    DevicePending {
        address: String,
        mac: Option<String>,
    },
    DeviceConnected {
        device_key: String,
        address: String,
        firmware: Option<String>,
        session: u64,
        /// A transport handshake is not evidence that the IMU reference restarted.
        /// Default false preserves the semantics of older recordings and other sources.
        #[serde(default)]
        preserve_calibration: bool,
    },
    SensorRegistered {
        device_key: String,
        sensor_id: u8,
        status: SensorStatus,
    },
    SensorMetadata {
        device_key: String,
        sensor_id: u8,
        imu_type: u8,
        data_type: u8,
        magnetometer_enabled: bool,
    },
    SensorState {
        device_key: String,
        sensor_id: u8,
        status: SensorStatus,
    },
    TransportState {
        device_key: String,
        timed_out: bool,
    },
    Sample {
        sample: TrackerSample,
    },
    Telemetry {
        device_key: String,
        sensor_id: Option<u8>,
        name: String,
    },
    UserAction {
        device_key: String,
        action: u8,
    },
    FlexValue {
        device_key: String,
        sensor_id: u8,
        session: u64,
        value: f32,
    },
    SensorTap {
        device_key: String,
        sensor_id: u8,
        tap: u8,
    },
    Rejected {
        address: String,
        reason: String,
    },
    Ignored {
        address: String,
        packet_id: u32,
        reason: String,
    },
    CompatibilityFallback {
        address: String,
        reason: String,
    },
}

impl EventKind {
    pub fn is_noisy(&self) -> bool {
        matches!(
            self,
            Self::Sample { .. }
                | Self::FlexValue { .. }
                | Self::Telemetry { .. }
                | Self::Ignored { .. }
        )
    }
}

pub mod alignment;
pub mod autobone;
pub mod constraints;
pub mod flex;
pub mod gestures;
pub mod legs;
pub mod localizer;
pub mod velocity;
