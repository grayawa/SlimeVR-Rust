//! SolarXR encoding uses the repository's generated bindings; no alternate GUI protocol.
use crate::receiver::DeviceState;
use slimevr_core::{
    pose::{PoseConfig, PoseSnapshot},
    skeleton::{BodyPosition as B, HeadPose, SkeletonConfig},
    Quaternion as Q, SensorStatus, Vector3 as V,
};
use solarxr_protocol::{self as sx, data_feed as df, datatypes as dt, flatbuffers as fb, rpc};
use std::collections::BTreeMap;

pub fn quat(q: Q) -> dt::math::Quat {
    dt::math::Quat::new(q.x, q.y, q.z, q.w)
}
pub fn vector(v: V) -> dt::math::Vec3f {
    dt::math::Vec3f::new(v.x, v.y, v.z)
}
pub fn from_quat(q: &dt::math::Quat) -> Q {
    Q {
        w: q.w(),
        x: q.x(),
        y: q.y(),
        z: q.z(),
    }
}
pub fn body(b: B) -> dt::BodyPart {
    let name = serde_json::to_value(b)
        .unwrap()
        .as_str()
        .unwrap()
        .to_ascii_uppercase();
    dt::BodyPart::ENUM_VALUES
        .iter()
        .copied()
        .find(|v| v.variant_name() == Some(name.as_str()))
        .unwrap_or(dt::BodyPart::NONE)
}
pub fn from_body(b: dt::BodyPart) -> Result<B, String> {
    serde_json::from_value(serde_json::Value::String(
        b.variant_name()
            .ok_or("unknown body part")?
            .to_ascii_lowercase(),
    ))
    .map_err(|_| "body part cannot be assigned".into())
}
pub fn rpc_frame(
    kind: rpc::RpcMessage,
    tx: u32,
    build: impl FnOnce(&mut fb::FlatBufferBuilder<'_>) -> fb::WIPOffset<fb::UnionWIPOffset>,
) -> Vec<u8> {
    let mut f = fb::FlatBufferBuilder::new();
    let message = build(&mut f);
    let id = dt::TransactionId::new(tx);
    let h = rpc::RpcMessageHeader::create(
        &mut f,
        &rpc::RpcMessageHeaderArgs {
            tx_id: Some(&id),
            message_type: kind,
            message: Some(message),
        },
    );
    let headers = f.create_vector(&[h]);
    let bundle = sx::MessageBundle::create(
        &mut f,
        &sx::MessageBundleArgs {
            rpc_msgs: Some(headers),
            ..Default::default()
        },
    );
    f.finish(bundle, None);
    f.finished_data().to_vec()
}
pub fn reset_frame(
    tx: u32,
    kind: rpc::ResetType,
    parts: &[dt::BodyPart],
    progress: u64,
    duration: u64,
    done: bool,
) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::ResetResponse, tx, |f| {
        let parts = f.create_vector(parts);
        rpc::ResetResponse::create(
            f,
            &rpc::ResetResponseArgs {
                reset_type: kind,
                status: if done {
                    rpc::ResetStatus::FINISHED
                } else {
                    rpc::ResetStatus::STARTED
                },
                body_parts: Some(parts),
                progress: progress.min(i32::MAX as u64) as i32,
                duration: duration.min(i32::MAX as u64) as i32,
            },
        )
        .as_union_value()
    })
}
pub fn pause_frame(tx: u32, paused: bool) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::TrackingPauseStateResponse, tx, |f| {
        rpc::TrackingPauseStateResponse::create(
            f,
            &rpc::TrackingPauseStateResponseArgs {
                trackingPaused: paused,
            },
        )
        .as_union_value()
    })
}
pub fn skeleton_parts<'a>(
    f: &mut fb::FlatBufferBuilder<'a>,
    config: SkeletonConfig,
) -> fb::WIPOffset<fb::Vector<'a, fb::ForwardsUOffset<rpc::SkeletonPart<'a>>>> {
    let c = serde_json::to_value(config).unwrap();
    let parts: BTreeMap<_, _> = offset_fields().into_iter().collect();
    let offsets: Vec<_> = parts
        .into_iter()
        .map(|(bone, key)| {
            rpc::SkeletonPart::create(
                f,
                &rpc::SkeletonPartArgs {
                    bone: rpc::SkeletonBone(bone),
                    value: c[key].as_f64().unwrap() as f32,
                },
            )
        })
        .collect();
    f.create_vector(&offsets)
}
pub fn offset_fields() -> Vec<(u8, &'static str)> {
    vec![
        (1, "head_shift"),
        (2, "neck_length"),
        (3, "chest_length"),
        (4, "chest_offset"),
        (5, "waist_length"),
        (6, "hip_length"),
        (7, "hip_offset"),
        (8, "hips_width"),
        (9, "upper_leg_length"),
        (10, "lower_leg_length"),
        (11, "foot_length"),
        (12, "foot_shift"),
        (13, "skeleton_offset"),
        (14, "shoulders_distance"),
        (15, "shoulders_width"),
        (16, "upper_arm_length"),
        (17, "lower_arm_length"),
        (18, "hand_y"),
        (19, "hand_z"),
        (20, "elbow_offset"),
        (21, "upper_chest_length"),
    ]
}
pub fn skeleton_frame(tx: u32, c: &PoseConfig) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::SkeletonConfigResponse, tx, |f| {
        let parts = skeleton_parts(f, c.skeleton);
        rpc::SkeletonConfigResponse::create(
            f,
            &rpc::SkeletonConfigResponseArgs {
                skeleton_parts: Some(parts),
                user_height: c
                    .hmd_height
                    .unwrap_or_else(|| slimevr_core::autobone::skeleton_height(c.skeleton)),
            },
        )
        .as_union_value()
    })
}
pub fn unknown_frame(mac: &str) -> Vec<u8> {
    rpc_frame(
        rpc::RpcMessage::UnknownDeviceHandshakeNotification,
        0,
        |f| {
            let mac = f.create_string(mac);
            rpc::UnknownDeviceHandshakeNotification::create(
                f,
                &rpc::UnknownDeviceHandshakeNotificationArgs {
                    mac_address: Some(mac),
                },
            )
            .as_union_value()
        },
    )
}
pub fn height_frame(p: &PoseSnapshot) -> Vec<u8> {
    use slimevr_core::gestures::HeightStatus as H;
    let status = match p.height_status {
        H::Idle => 0,
        H::RecordingFloor => 1,
        H::WaitingControllerPitch => 2,
        H::WaitingRise => 3,
        H::WaitingForwardLook => 4,
        H::RecordingHeight => 5,
        H::Done => 6,
        H::TooHigh => 7,
        H::TooSmall => 8,
        H::Timeout => 9,
    };
    rpc_frame(rpc::RpcMessage::UserHeightRecordingStatusResponse, 0, |f| {
        rpc::UserHeightRecordingStatusResponse::create(
            f,
            &rpc::UserHeightRecordingStatusResponseArgs {
                hmdHeight: p.measured_height,
                status: rpc::UserHeightCalibrationStatus(status),
            },
        )
        .as_union_value()
    })
}
fn status(s: SensorStatus) -> dt::TrackerStatus {
    match s {
        SensorStatus::Ok => dt::TrackerStatus::OK,
        SensorStatus::Busy => dt::TrackerStatus::BUSY,
        SensorStatus::Occluded => dt::TrackerStatus::OCCLUDED,
        SensorStatus::Error => dt::TrackerStatus::ERROR,
        SensorStatus::TimedOut => dt::TrackerStatus::TIMED_OUT,
        SensorStatus::Disconnected => dt::TrackerStatus::DISCONNECTED,
    }
}
#[derive(Clone, Debug, Default)]
pub struct TrackerMask {
    info: bool,
    status: bool,
    rotation: bool,
    position: bool,
    acceleration: bool,
    linear: bool,
    temp: bool,
    reference: bool,
    aligned: bool,
    magnetic: bool,
}
impl TrackerMask {
    fn from_config(m: df::tracker::TrackerDataMask<'_>) -> Self {
        Self {
            info: m.info(),
            status: m.status(),
            rotation: m.rotation(),
            position: m.position(),
            acceleration: m.raw_acceleration(),
            linear: m.linear_acceleration(),
            temp: m.temp(),
            reference: m.rotation_reference_adjusted(),
            aligned: m.stay_aligned(),
            magnetic: m.raw_magnetic_vector(),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Feed {
    pub index: u8,
    pub minimum_ms: u64,
    pub devices: bool,
    pub trackers: bool,
    pub synthetic: bool,
    pub bones: bool,
    pub guards: bool,
    pub aligned_pose: bool,
    pub next_at: u64,
    pub tracker_mask: TrackerMask,
    pub synthetic_mask: TrackerMask,
}
impl Feed {
    pub fn from_config(index: u8, c: df::DataFeedConfig<'_>) -> Self {
        Self {
            index,
            minimum_ms: u64::from(c.minimum_time_since_last()).max(10),
            devices: c.data_mask().is_some_and(|m| m.device_data()),
            trackers: c.data_mask().is_some_and(|m| m.tracker_data().is_some()),
            synthetic: c.synthetic_trackers_mask().is_some(),
            bones: c.bone_mask(),
            guards: c.server_guards_mask(),
            aligned_pose: c.stay_aligned_pose_mask(),
            next_at: 0,
            tracker_mask: c
                .data_mask()
                .and_then(|m| m.tracker_data())
                .map(TrackerMask::from_config)
                .unwrap_or_default(),
            synthetic_mask: c
                .synthetic_trackers_mask()
                .map(TrackerMask::from_config)
                .unwrap_or_default(),
        }
    }
}
// Tracker/device component masks are honored for the stream's requested groups. Unknown telemetry remains absent.
pub fn data_frame(
    feed: &Feed,
    devices: &BTreeMap<String, DeviceState>,
    ids: &BTreeMap<String, u8>,
    pose: &PoseSnapshot,
    config: &PoseConfig,
    names: &BTreeMap<String, String>,
    external: &BTreeMap<B, HeadPose>,
) -> Vec<u8> {
    use df::{device_data as dd, server as sg, tracker as tr};
    use dt::hardware_info as hw;
    let mut f = fb::FlatBufferBuilder::new();
    let mut device_offsets = Vec::new();
    let mask = &feed.tracker_mask;
    if feed.devices || feed.trackers {
        for (key, d) in devices {
            let Some(id) = ids.get(key) else { continue };
            let device_id = dt::DeviceId::new(*id);
            let mut trackers = Vec::new();
            if feed.trackers {
                for (sensor_id, s) in &d.sensors {
                    let binding = config
                        .bindings
                        .iter()
                        .find(|b| b.device_key == *key && b.sensor_id == *sensor_id);
                    let p = pose
                        .trackers
                        .iter()
                        .find(|p| p.device_key == *key && p.sensor_id == *sensor_id);
                    let tracker_id = dt::TrackerId::create(
                        &mut f,
                        &dt::TrackerIdArgs {
                            device_id: Some(&device_id),
                            tracker_num: *sensor_id,
                        },
                    );
                    let display = f.create_string(
                        &d.display_name
                            .clone()
                            .unwrap_or_else(|| format!("SlimeVR {key} / {sensor_id}")),
                    );
                    let custom = names
                        .get(&format!("{key}/{sensor_id}"))
                        .map(|s| f.create_string(s));
                    let mounting = binding.map(|b| quat(b.mounting));
                    let mounting_reset = p.map(|p| quat(p.calibration.mount_rot_fix));
                    let info = tr::TrackerInfo::create(
                        &mut f,
                        &tr::TrackerInfoArgs {
                            body_part: binding.map_or(dt::BodyPart::NONE, |b| body(b.body)),
                            imu_type: hw::ImuType(u16::from(s.info.imu_type)),
                            mounting_orientation: mounting.as_ref(),
                            mounting_reset_orientation: mounting_reset.as_ref(),
                            editable: true,
                            is_imu: s.capabilities.is_imu,
                            is_computed: !s.capabilities.is_imu
                                && !d
                                    .display_name
                                    .as_deref()
                                    .is_some_and(|n| n.starts_with("VMC-Bone-")),
                            display_name: Some(display),
                            custom_name: custom,
                            data_support: hw::TrackerDataType(s.info.data_type),
                            magnetometer: match s.info.config {
                                Some(v) if v & 2 != 0 => {
                                    if v & 1 != 0 {
                                        dt::MagnetometerStatus::ENABLED
                                    } else {
                                        dt::MagnetometerStatus::DISABLED
                                    }
                                }
                                _ => dt::MagnetometerStatus::NOT_SUPPORTED,
                            },
                            ..Default::default()
                        },
                    );
                    let raw = s.rotation.as_ref().map(|q| quat(q.value));
                    let rotation = p.and_then(|p| p.rotation).map(quat);
                    let position = s.position.as_ref().map(|p| vector(p.value));
                    let acceleration = s.acceleration.as_ref().map(|a| vector(a.value));
                    let world = p
                        .and_then(|p| p.acceleration_world.as_ref())
                        .map(|a| vector(a.value));
                    let magnetic = s.magnetic_vector.as_ref().map(|v| vector(v.value));
                    let temp = s
                        .temperature
                        .as_ref()
                        .map(|t| dt::Temperature::new(t.value));
                    let aligned = p.and_then(|p| pose.alignment.get(&p.body)).map(|a| {
                        df::stay_aligned::StayAlignedTracker::create(
                            &mut f,
                            &df::stay_aligned::StayAlignedTrackerArgs {
                                yaw_correction_in_deg: a.yaw_correction.to_degrees(),
                                locked: a.rest.state == slimevr_core::alignment::RestState::AtRest,
                                locked_error_in_deg: a.errors[0].to_degrees(),
                                center_error_in_deg: a.errors[1].to_degrees(),
                                neighbor_error_in_deg: a.errors[2].to_degrees(),
                            },
                        )
                    });
                    trackers.push(tr::TrackerData::create(
                        &mut f,
                        &tr::TrackerDataArgs {
                            tracker_id: Some(tracker_id),
                            info: mask.info.then_some(info),
                            status: if mask.status {
                                status(s.status)
                            } else {
                                dt::TrackerStatus::NONE
                            },
                            rotation: raw.as_ref().filter(|_| mask.rotation),
                            rotation_reference_adjusted: rotation
                                .as_ref()
                                .filter(|_| mask.reference),
                            position: position.as_ref().filter(|_| mask.position),
                            raw_acceleration: acceleration.as_ref().filter(|_| mask.acceleration),
                            raw_magnetic_vector: magnetic.as_ref().filter(|_| mask.magnetic),
                            linear_acceleration: world.as_ref().filter(|_| mask.linear),
                            temp: temp.as_ref().filter(|_| mask.temp),
                            stay_aligned: aligned.filter(|_| mask.aligned),
                            ..Default::default()
                        },
                    ));
                }
            }
            let trackers = f.create_vector(&trackers);
            let identifier = f.create_string(key);
            let firmware = d.handshake.firmware.as_deref().map(|s| f.create_string(s));
            let display = f.create_string(d.display_name.as_deref().unwrap_or(
                if d.origin == crate::receiver::Origin::Hid {
                    "SlimeVR HID tracker"
                } else {
                    "SlimeVR UDP tracker"
                },
            ));
            let address = d
                .handshake
                .mac
                .as_ref()
                .and_then(|m| u64::from_str_radix(&m.replace(':', ""), 16).ok())
                .map(hw::HardwareAddress::new);
            let ip = match d.address.ip() {
                std::net::IpAddr::V4(ip) if d.origin == crate::receiver::Origin::Udp => {
                    Some(dt::Ipv4Address::new(u32::from(ip)))
                }
                _ => None,
            };
            let firmware_date = d.firmware_date.as_deref().map(|date| f.create_string(date));
            let hardware = hw::HardwareInfo::create(
                &mut f,
                &hw::HardwareInfoArgs {
                    display_name: Some(display),
                    firmware_version: firmware,
                    hardware_address: address.as_ref(),
                    hardware_identifier: Some(identifier),
                    ip_address: ip.as_ref(),
                    network_protocol_version: (d.origin == crate::receiver::Origin::Udp)
                        .then_some(d.handshake.protocol_version.min(u16::MAX as u32) as u16),
                    mcu_id: hw::McuType(d.handshake.mcu_type as u16),
                    official_board_type: hw::BoardType(d.handshake.board_type as u16),
                    firmware_date,
                    ..Default::default()
                },
            );
            let hardware_status = hw::HardwareStatus::create(
                &mut f,
                &hw::HardwareStatusArgs {
                    ping: d
                        .half_rtt_ms
                        .as_ref()
                        .map(|v| v.value.min(u16::MAX as u64) as u16),
                    rssi: d.rssi.as_ref().map(|v| v.value),
                    battery_voltage: d.battery_voltage.as_ref().map(|v| v.value),
                    battery_runtime_estimate: d
                        .battery_runtime
                        .map(|v| v.min(i64::MAX as u64) as i64),
                    packet_loss: d.packet_loss,
                    packets_received: d.packets_received,
                    packets_lost: d.packets_lost,
                    battery_pct_estimate: d
                        .battery_fraction
                        .as_ref()
                        .map(|v| (v.value.clamp(0.0, 1.0) * 100.0).round() as u8),
                    ..Default::default()
                },
            );
            device_offsets.push(dd::DeviceData::create(
                &mut f,
                &dd::DeviceDataArgs {
                    id: Some(&device_id),
                    hardware_info: feed.devices.then_some(hardware),
                    hardware_status: feed.devices.then_some(hardware_status),
                    trackers: Some(trackers),
                    ..Default::default()
                },
            ));
        }
    }
    if (feed.devices || feed.trackers) && !external.is_empty() {
        let device_id = dt::DeviceId::new(0);
        let mut trackers = Vec::new();
        if feed.trackers {
            for (part, source) in external {
                let number = match part {
                    B::Head => 0,
                    B::LeftHand => 1,
                    B::RightHand => 2,
                    _ => continue,
                };
                let id = dt::TrackerId::create(
                    &mut f,
                    &dt::TrackerIdArgs {
                        device_id: Some(&device_id),
                        tracker_num: number,
                    },
                );
                let label = f.create_string(match part {
                    B::Head => "External HMD",
                    B::LeftHand => "External left controller",
                    _ => "External right controller",
                });
                let info = tr::TrackerInfo::create(
                    &mut f,
                    &tr::TrackerInfoArgs {
                        body_part: body(*part),
                        is_hmd: *part == B::Head,
                        display_name: Some(label),
                        ..Default::default()
                    },
                );
                let rotation = if *part == B::Head {
                    pose.skeleton
                        .computed
                        .get("head")
                        .map_or(source.rotation, |p| p.rotation)
                } else {
                    source.rotation
                };
                let q = quat(rotation);
                let v = source.position.map(vector);
                trackers.push(tr::TrackerData::create(
                    &mut f,
                    &tr::TrackerDataArgs {
                        tracker_id: Some(id),
                        info: mask.info.then_some(info),
                        status: if mask.status {
                            dt::TrackerStatus::OK
                        } else {
                            dt::TrackerStatus::NONE
                        },
                        rotation: mask.rotation.then_some(&q),
                        rotation_reference_adjusted: mask.reference.then_some(&q),
                        position: v.as_ref().filter(|_| mask.position),
                        ..Default::default()
                    },
                ));
            }
        }
        let trackers = f.create_vector(&trackers);
        let label = f.create_string("External pose sources");
        let hardware = hw::HardwareInfo::create(
            &mut f,
            &hw::HardwareInfoArgs {
                display_name: Some(label),
                ..Default::default()
            },
        );
        device_offsets.push(dd::DeviceData::create(
            &mut f,
            &dd::DeviceDataArgs {
                id: Some(&device_id),
                hardware_info: feed.devices.then_some(hardware),
                trackers: Some(trackers),
                ..Default::default()
            },
        ));
    }
    let device_offsets = f.create_vector(&device_offsets);
    let mut synthetic = Vec::new();
    if feed.synthetic {
        let mask = &feed.synthetic_mask;
        for (i, (name, p)) in pose.skeleton.computed.iter().enumerate() {
            let part = match name.as_str() {
                "left_knee" => dt::BodyPart::LEFT_LOWER_LEG,
                "right_knee" => dt::BodyPart::RIGHT_LOWER_LEG,
                "left_elbow" => dt::BodyPart::LEFT_LOWER_ARM,
                "right_elbow" => dt::BodyPart::RIGHT_LOWER_ARM,
                _ => serde_json::from_value::<B>(serde_json::Value::String(name.clone()))
                    .map(body)
                    .unwrap_or(dt::BodyPart::NONE),
            };
            let display = f.create_string(name);
            let info = tr::TrackerInfo::create(
                &mut f,
                &tr::TrackerInfoArgs {
                    body_part: part,
                    is_computed: true,
                    display_name: Some(display),
                    ..Default::default()
                },
            );
            let id = dt::TrackerId::create(
                &mut f,
                &dt::TrackerIdArgs {
                    tracker_num: i as u8,
                    ..Default::default()
                },
            );
            let q = quat(p.rotation);
            let v = vector(p.position);
            synthetic.push(tr::TrackerData::create(
                &mut f,
                &tr::TrackerDataArgs {
                    tracker_id: Some(id),
                    info: mask.info.then_some(info),
                    status: if mask.status {
                        dt::TrackerStatus::OK
                    } else {
                        dt::TrackerStatus::NONE
                    },
                    position: mask.position.then_some(&v),
                    rotation: mask.rotation.then_some(&q),
                    ..Default::default()
                },
            ));
        }
    }
    let synthetic = f.create_vector(&synthetic);
    let mut bones = Vec::new();
    if feed.bones {
        for (name, b) in &pose.skeleton.bones {
            let part = match name.as_str() {
                "left_hip" => dt::BodyPart::LEFT_HIP,
                "right_hip" => dt::BodyPart::RIGHT_HIP,
                _ => {
                    let Ok(body_position) =
                        serde_json::from_value::<B>(serde_json::Value::String(name.clone()))
                    else {
                        continue;
                    };
                    body(body_position)
                }
            };
            let q = quat(b.rotation);
            let v = vector(b.head);
            bones.push(df::Bone::create(
                &mut f,
                &df::BoneArgs {
                    body_part: part,
                    rotation_g: Some(&q),
                    head_position_g: Some(&v),
                    bone_length: b.length,
                },
            ));
        }
    }
    let bones = f.create_vector(&bones);
    let positioned = |body| {
        external.get(&body).is_some_and(|p| p.position.is_some())
            || pose.trackers.iter().any(|t| {
                t.body == body
                    && t.position.is_some()
                    && matches!(t.status, SensorStatus::Ok | SensorStatus::Busy)
            })
    };
    let guards = sg::ServerGuards::create(
        &mut f,
        &sg::ServerGuardsArgs {
            canDoYawReset: pose.last_full_reset_ms.is_some(),
            canDoMounting: pose
                .last_full_reset_ms
                .is_some_and(|at| pose.at_ms.saturating_sub(at) < 120000),
            canDoUserHeightCalibration: !pose.paused
                && positioned(B::Head)
                && (positioned(B::LeftHand) || positioned(B::RightHand)),
        },
    );
    let aligned_pose = if feed.aligned_pose {
        let qs = pose
            .trackers
            .iter()
            .filter_map(|p| p.calibrated.map(|q| (p.body, q)))
            .collect();
        let angles = slimevr_core::alignment::RelaxedPose::from_rotations(&qs);
        Some(df::stay_aligned::StayAlignedPose::create(
            &mut f,
            &df::stay_aligned::StayAlignedPoseArgs {
                upper_leg_angle_in_deg: angles.upper_leg_degrees,
                lower_leg_angle_in_deg: angles.lower_leg_degrees,
                foot_angle_in_deg: angles.foot_degrees,
            },
        ))
    } else {
        None
    };
    let update = df::DataFeedUpdate::create(
        &mut f,
        &df::DataFeedUpdateArgs {
            index: feed.index,
            devices: Some(device_offsets),
            synthetic_trackers: Some(synthetic),
            bones: Some(bones),
            server_guards: feed.guards.then_some(guards),
            stay_aligned_pose: aligned_pose,
        },
    );
    let header = df::DataFeedMessageHeader::create(
        &mut f,
        &df::DataFeedMessageHeaderArgs {
            message_type: df::DataFeedMessage::DataFeedUpdate,
            message: Some(update.as_union_value()),
        },
    );
    let headers = f.create_vector(&[header]);
    let bundle = sx::MessageBundle::create(
        &mut f,
        &sx::MessageBundleArgs {
            data_feed_msgs: Some(headers),
            ..Default::default()
        },
    );
    f.finish(bundle, None);
    f.finished_data().to_vec()
}

pub fn tap_frame(device: u8, sensor: u8) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::TapDetectionSetupNotification, 0, |f| {
        let device = dt::DeviceId::new(device);
        let tracker = dt::TrackerId::create(
            f,
            &dt::TrackerIdArgs {
                device_id: Some(&device),
                tracker_num: sensor,
            },
        );
        rpc::TapDetectionSetupNotification::create(
            f,
            &rpc::TapDetectionSetupNotificationArgs {
                tracker_id: Some(tracker),
            },
        )
        .as_union_value()
    })
}
