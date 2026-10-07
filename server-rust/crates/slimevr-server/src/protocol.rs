//! SlimeVR UDP, pinned to the Java/Kotlin receiver at 83941fd3.
use serde::{Deserialize, Serialize};
use slimevr_core::{Quaternion, Vector3};
use thiserror::Error;

pub const MAX_DATAGRAM: usize = 1472;
pub const SERVER_FEATURES: u8 = 0b11; // Normal and compact bundles, both implemented.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{reason} at byte {offset}")]
pub struct ParseError {
    pub offset: usize,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handshake {
    pub board_type: u32,
    pub imu_type: u32,
    pub mcu_type: u32,
    pub protocol_version: u32,
    pub firmware: Option<String>,
    pub mac: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SensorInfo {
    pub sensor_id: u8,
    pub status: u8,
    pub imu_type: u8,
    pub config: Option<u16>,
    pub rest_calibrated: Option<bool>,
    pub body_position: Option<u8>,
    pub data_type: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Packet {
    Heartbeat,
    Handshake {
        info: Handshake,
    },
    SensorInfo {
        info: SensorInfo,
    },
    Rotation {
        sensor_id: u8,
        rotation: Quaternion,
        data_type: u8,
        calibration: Option<u8>,
        acceleration: Option<Vector3>,
        fallback: bool,
    },
    Acceleration {
        sensor_id: u8,
        acceleration: Vector3,
    },
    Ping {
        id: i32,
    },
    Battery {
        voltage: Option<f32>,
        fraction: f32,
    },
    Signal {
        sensor_id: u8,
        rssi: i8,
    },
    Temperature {
        sensor_id: u8,
        temperature: f32,
    },
    Features {
        flags: Vec<u8>,
    },
    Error {
        sensor_id: u8,
        code: u8,
    },
    Tap {
        sensor_id: u8,
        tap: u8,
    },
    UserAction {
        action: u8,
    },
    Position {
        sensor_id: u8,
        position: Vector3,
    },
    Flex {
        sensor_id: u8,
        value: f32,
    },
    ConfigAck {
        sensor_id: u8,
        config_type: u16,
    },
    ProtocolChange {
        protocol: u8,
        version: u8,
    },
    /// Bytes are retained in recordings; serial content is not printed by default.
    Serial {
        text: String,
    },
    Unknown {
        packet_id: u32,
        payload_bytes: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Datagram {
    pub packet_id: u32,
    /// Java reads a signed Long; preserve that comparison behavior explicitly.
    pub sequence: i64,
    pub packets: Vec<Packet>,
    pub warnings: Vec<String>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    base: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            pos: 0,
            base,
        }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
    fn error(&self, reason: impl Into<String>) -> ParseError {
        ParseError {
            offset: self.base + self.pos,
            reason: reason.into(),
        }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        if n > self.remaining() {
            return Err(self.error(format!(
                "truncated field: need {n}, have {}",
                self.remaining()
            )));
        }
        let bytes = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ParseError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, ParseError> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, ParseError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, ParseError> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn float(&mut self) -> Result<f32, ParseError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn safe_float(&mut self, warnings: &mut Vec<String>) -> Result<f32, ParseError> {
        let x = self.float()?;
        if x.is_nan() {
            warnings.push("NaN scalar replaced with zero (Java compatibility)".into());
            Ok(0.0)
        } else if !x.is_finite() {
            Err(self.error("infinite scalar"))
        } else {
            Ok(x)
        }
    }
    fn vector(&mut self, warnings: &mut Vec<String>) -> Result<Vector3, ParseError> {
        Ok(Vector3 {
            x: self.safe_float(warnings)?,
            y: self.safe_float(warnings)?,
            z: self.safe_float(warnings)?,
        })
    }
    fn quaternion(&mut self, warnings: &mut Vec<String>) -> Result<(Quaternion, bool), ParseError> {
        let x = self.float()?;
        let y = self.float()?;
        let z = self.float()?;
        let w = self.float()?;
        if [x, y, z, w].iter().any(|v| v.is_nan()) || [x, y, z, w].iter().all(|v| *v == 0.0) {
            warnings.push(
                "invalid floating quaternion replaced with identity (Java compatibility)".into(),
            );
            Ok((Quaternion::IDENTITY, true))
        } else if [x, y, z, w].iter().any(|v| !v.is_finite()) {
            Err(self.error("infinite quaternion"))
        } else {
            let rotation = Quaternion { w, x, y, z };
            let server = rotation.udp_to_server();
            if [server.w, server.x, server.y, server.z]
                .iter()
                .any(|v| !v.is_finite())
            {
                return Err(self.error("quaternion overflows coordinate conversion"));
            }
            Ok((rotation, false))
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<Datagram, ParseError> {
    if bytes.len() > MAX_DATAGRAM {
        return Err(ParseError {
            offset: 0,
            reason: "datagram exceeds 1472-byte compatibility limit".into(),
        });
    }
    let mut r = Reader::new(bytes, 0);
    let packet_id = r.u32()?;
    let sequence = r.i64()?;
    let mut warnings = Vec::new();
    let mut packets = Vec::new();
    if packet_id == 100 || packet_id == 101 {
        while r.remaining() > 0 {
            let length = if packet_id == 100 {
                r.u16()? as usize
            } else {
                r.u8()? as usize
            };
            if length == 0 {
                continue;
            }
            // Reject oversize lengths atomically, unlike Java's clamping of malformed bundles.
            let base = r.pos;
            let body = r.take(length)?;
            let mut sub = Reader::new(body, base);
            let id = if packet_id == 100 {
                sub.u32()?
            } else {
                sub.u8()? as u32
            };
            packets.push(parse_packet(id, &mut sub, &mut warnings)?);
        }
    } else {
        packets.push(parse_packet(packet_id, &mut r, &mut warnings)?);
    }
    Ok(Datagram {
        packet_id,
        sequence,
        packets,
        warnings,
    })
}

fn parse_packet(
    id: u32,
    r: &mut Reader<'_>,
    warnings: &mut Vec<String>,
) -> Result<Packet, ParseError> {
    Ok(match id {
        0 => Packet::Heartbeat,
        3 => {
            let mut info = Handshake {
                board_type: 0,
                imu_type: 0,
                mcu_type: 0,
                protocol_version: 0,
                firmware: None,
                mac: None,
            };
            if r.remaining() > 0 {
                if r.remaining() >= 4 {
                    info.board_type = r.u32()?;
                }
                if r.remaining() >= 4 {
                    info.imu_type = r.u32()?;
                }
                if r.remaining() >= 4 {
                    info.mcu_type = r.u32()?;
                }
                if r.remaining() >= 12 {
                    r.take(12)?;
                }
                if r.remaining() >= 4 {
                    info.protocol_version = r.u32()?;
                }
                let length = if r.remaining() > 0 {
                    r.u8()? as usize
                } else {
                    0
                };
                // Java's length includes the NUL and consumes only through it.
                let mut firmware = String::new();
                for _ in 0..length {
                    let c = r.u8()?;
                    if c == 0 {
                        break;
                    }
                    firmware.push(char::from(c));
                }
                info.firmware = Some(firmware);
                if r.remaining() >= 6 {
                    let mac = r.take(6)?;
                    if mac.iter().any(|b| *b != 0) {
                        info.mac = Some(
                            mac.iter()
                                .map(|b| format!("{b:02X}"))
                                .collect::<Vec<_>>()
                                .join(":"),
                        );
                    }
                }
            }
            Packet::Handshake { info }
        }
        15 => {
            let sensor_id = r.u8()?;
            let status = r.u8()?;
            let imu_type = if r.remaining() > 0 { r.u8()? } else { 0 };
            let config = if r.remaining() >= 2 {
                Some(r.u16()?)
            } else {
                None
            };
            let rest_calibrated = if r.remaining() > 0 {
                Some(r.u8()? != 0)
            } else {
                None
            };
            let body_position = if r.remaining() > 0 {
                Some(r.u8()?)
            } else {
                None
            };
            let data_type = if r.remaining() > 0 { r.u8()? } else { 0 };
            Packet::SensorInfo {
                info: SensorInfo {
                    sensor_id,
                    status,
                    imu_type,
                    config,
                    rest_calibrated,
                    body_position,
                    data_type,
                },
            }
        }
        1 | 16 | 17 => {
            let sensor_id = if id == 17 {
                r.u8()?
            } else if id == 16 {
                1
            } else {
                0
            };
            let data_type = if id == 17 { r.u8()? } else { 1 };
            let (rotation, fallback) = r.quaternion(warnings)?;
            let calibration = if id == 17 { Some(r.u8()?) } else { None };
            Packet::Rotation {
                sensor_id,
                rotation,
                data_type,
                calibration,
                acceleration: None,
                fallback,
            }
        }
        23 => {
            let sensor_id = r.u8()?;
            let scale = 1.0 / 32768.0;
            let x = r.i16()? as f32 * scale;
            let y = r.i16()? as f32 * scale;
            let z = r.i16()? as f32 * scale;
            let w = r.i16()? as f32 * scale;
            let norm = (w * w + x * x + y * y + z * z).sqrt();
            if norm == 0.0 {
                return Err(r.error("zero compact quaternion"));
            }
            let rotation = Quaternion {
                w: w / norm,
                x: x / norm,
                y: y / norm,
                z: z / norm,
            };
            let acceleration = Vector3 {
                x: r.i16()? as f32 / 128.0,
                y: r.i16()? as f32 / 128.0,
                z: r.i16()? as f32 / 128.0,
            };
            Packet::Rotation {
                sensor_id,
                rotation,
                data_type: 1,
                calibration: None,
                acceleration: Some(acceleration),
                fallback: false,
            }
        }
        4 => {
            let acceleration = r.vector(warnings)?;
            let sensor_id = if r.remaining() > 0 { r.u8()? } else { 0 };
            Packet::Acceleration {
                sensor_id,
                acceleration,
            }
        }
        10 => Packet::Ping {
            id: r.u32()? as i32,
        },
        11 => {
            let length = r.u32()? as usize;
            let data = r.take(length)?;
            Packet::Serial {
                text: data.iter().map(|b| char::from(*b)).collect(),
            }
        }
        12 => {
            let voltage = if r.remaining() >= 8 {
                Some(r.safe_float(warnings)?)
            } else {
                None
            };
            let fraction = r.safe_float(warnings)?;
            Packet::Battery { voltage, fraction }
        }
        13 => Packet::Tap {
            sensor_id: r.u8()?,
            tap: r.u8()?,
        },
        14 => Packet::Error {
            sensor_id: r.u8()?,
            code: r.u8()?,
        },
        19 => Packet::Signal {
            sensor_id: r.u8()?,
            rssi: r.u8()? as i8,
        },
        20 => Packet::Temperature {
            sensor_id: r.u8()?,
            temperature: r.safe_float(warnings)?,
        },
        21 => Packet::UserAction { action: r.u8()? },
        22 => Packet::Features {
            flags: r.take(r.remaining())?.to_vec(),
        },
        24 => Packet::ConfigAck {
            sensor_id: r.u8()?,
            config_type: r.u16()?,
        },
        26 => Packet::Flex {
            sensor_id: r.u8()?,
            value: r.safe_float(warnings)?,
        },
        27 => {
            let sensor_id = r.u8()?;
            Packet::Position {
                sensor_id,
                position: r.vector(warnings)?,
            }
        }
        200 => Packet::ProtocolChange {
            protocol: r.u8()?,
            version: r.u8()?,
        },
        _ => Packet::Unknown {
            packet_id: id,
            payload_bytes: r.remaining(),
        },
    })
}

pub fn header(packet_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12 + payload.len());
    bytes.extend(packet_id.to_be_bytes());
    bytes.extend(0i64.to_be_bytes());
    bytes.extend(payload);
    bytes
}

pub fn handshake_response() -> Vec<u8> {
    let mut bytes = vec![0; 64];
    bytes[0] = 3;
    bytes[1..13].copy_from_slice(b"Hey OVR =D 5");
    bytes
}

pub fn sensor_info_response(sensor: u8, status: u8) -> Vec<u8> {
    let mut bytes = 15u32.to_be_bytes().to_vec();
    bytes.extend([sensor, status]);
    bytes
}
