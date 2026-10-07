//! Owned UI data decoded from the repository's verified FlatBuffers bindings.
use serde::Serialize;
use solarxr_protocol::{self as sx, data_feed as df, datatypes as dt, flatbuffers as fb, rpc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub struct TrackerKey {
    pub device: u8,
    pub sensor: u8,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Tracker {
    pub key: TrackerKey,
    pub name: String,
    pub custom_name: Option<String>,
    pub body: u8,
    pub status: u8,
    pub hardware: String,
    pub battery: Option<u8>,
    pub battery_runtime_us: Option<u64>,
    pub ping: Option<u16>,
    pub firmware: String,
    pub firmware_date: String,
    pub manufacturer: String,
    pub display_name: String,
    pub address: Option<String>,
    pub board_name: String,
    pub network_version: Option<u16>,
    pub board: u16,
    pub rssi: Option<i16>,
    pub voltage: Option<f32>,
    pub temperature: Option<f32>,
    pub packet_loss: Option<f32>,
    pub tps: Option<u16>,
    pub rotation: Option<[f32; 4]>,
    pub raw_rotation: Option<[f32; 4]>,
    pub position: Option<[f32; 3]>,
    pub acceleration: Option<[f32; 3]>,
    pub linear_acceleration: Option<[f32; 3]>,
    pub magnetic: Option<[f32; 3]>,
    pub mounting: Option<[f32; 4]>,
    pub imu: String,
    pub is_imu: bool,
    pub editable: bool,
    pub computed: bool,
    pub magnetometer: u8,
    pub yaw_correction: Option<f32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Feed {
    pub trackers: Vec<Tracker>,
    pub bones: Vec<Bone>,
    pub can_yaw: bool,
    pub can_mount: bool,
    pub can_height: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Bone {
    pub body: u8,
    pub head: [f32; 3],
    pub tail: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ResetKind {
    Full,
    Yaw,
    Mounting,
}
impl ResetKind {
    pub fn wire(self) -> rpc::ResetType {
        match self {
            Self::Full => rpc::ResetType::Full,
            Self::Yaw => rpc::ResetType::Yaw,
            Self::Mounting => rpc::ResetType::Mounting,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "reset-full",
            Self::Yaw => "reset-yaw",
            Self::Mounting => "reset-mounting",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    Reset(ResetKind),
    Pause(bool),
    Assign {
        key: TrackerKey,
        body: u8,
    },
    ReadSettings,
    ReadVrchat,
    MuteVrchat(String),
    Rpc {
        name: String,
        value: serde_json::Value,
    },
    PubSub(serde_json::Value),
    Batch(Vec<(String, serde_json::Value)>),
    FeedConfig(u16, u16, bool),
}

#[derive(Clone, Debug, Serialize)]
pub struct ResetProgress {
    pub tx: u32,
    pub kind: u8,
    pub done: bool,
    pub progress_ms: i32,
    pub duration_ms: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Settings {
    pub steamvr_roles: Vec<String>,
    pub filter: Option<String>,
    pub filter_amount: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VrchatRow {
    pub key: String,
    pub label: String,
    pub valid: bool,
    pub muted: bool,
    pub current: String,
    pub recommended: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Vrchat {
    pub supported: bool,
    pub rows: Vec<VrchatRow>,
}

#[derive(Debug)]
pub enum Update {
    Feed(Feed),
    Bones(Vec<Bone>),
    Reset(ResetProgress),
    Paused(bool),
    Settings(Settings),
    Vrchat(Vrchat),
    Heartbeat,
}

fn rpc_frame(
    kind: rpc::RpcMessage,
    tx: u32,
    build: impl FnOnce(&mut fb::FlatBufferBuilder<'_>) -> fb::WIPOffset<fb::UnionWIPOffset>,
) -> Vec<u8> {
    let mut f = fb::FlatBufferBuilder::new();
    let message = build(&mut f);
    let id = dt::TransactionId::new(tx);
    let header = rpc::RpcMessageHeader::create(
        &mut f,
        &rpc::RpcMessageHeaderArgs {
            tx_id: Some(&id),
            message_type: kind,
            message: Some(message),
        },
    );
    let headers = f.create_vector(&[header]);
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

pub fn encode(command: &Command, tx: u32) -> Vec<u8> {
    match command {
        Command::FeedConfig(telemetry, bones, enabled) => {
            subscribe_config(*telemetry, *bones, *enabled)
        }
        Command::Batch(requests) => {
            crate::rpc_generated::encode_rpc_batch(requests, tx).unwrap_or_default()
        }
        Command::Rpc { name, value } => {
            crate::rpc_generated::encode_rpc(name, value, tx).unwrap_or_default()
        }
        Command::PubSub(value) => crate::rpc_generated::encode_pubsub(value).unwrap_or_default(),
        Command::Reset(kind) => rpc_frame(rpc::RpcMessage::ResetRequest, tx, |f| {
            rpc::ResetRequest::create(
                f,
                &rpc::ResetRequestArgs {
                    reset_type: kind.wire(),
                    ..Default::default()
                },
            )
            .as_union_value()
        }),
        Command::Pause(paused) => rpc_frame(rpc::RpcMessage::SetPauseTrackingRequest, tx, |f| {
            rpc::SetPauseTrackingRequest::create(
                f,
                &rpc::SetPauseTrackingRequestArgs {
                    pauseTracking: *paused,
                },
            )
            .as_union_value()
        }),
        Command::Assign { key, body } => {
            rpc_frame(rpc::RpcMessage::AssignTrackerRequest, tx, |f| {
                let device = dt::DeviceId::new(key.device);
                let id = dt::TrackerId::create(
                    f,
                    &dt::TrackerIdArgs {
                        device_id: Some(&device),
                        tracker_num: key.sensor,
                    },
                );
                rpc::AssignTrackerRequest::create(
                    f,
                    &rpc::AssignTrackerRequestArgs {
                        tracker_id: Some(id),
                        body_position: dt::BodyPart(*body),
                        ..Default::default()
                    },
                )
                .as_union_value()
            })
        }
        Command::ReadSettings => rpc_frame(rpc::RpcMessage::SettingsRequest, tx, |f| {
            rpc::SettingsRequest::create(f, &Default::default()).as_union_value()
        }),
        Command::ReadVrchat => rpc_frame(rpc::RpcMessage::VRCConfigStateRequest, tx, |f| {
            rpc::VRCConfigStateRequest::create(f, &Default::default()).as_union_value()
        }),
        Command::MuteVrchat(key) => {
            rpc_frame(rpc::RpcMessage::VRCConfigSettingToggleMute, tx, |f| {
                let key = f.create_string(key);
                rpc::VRCConfigSettingToggleMute::create(
                    f,
                    &rpc::VRCConfigSettingToggleMuteArgs { key: Some(key) },
                )
                .as_union_value()
            })
        }
    }
}

pub fn pause_request(tx: u32) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::TrackingPauseStateRequest, tx, |f| {
        rpc::TrackingPauseStateRequest::create(f, &Default::default()).as_union_value()
    })
}

pub fn heartbeat(tx: u32) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::HeartbeatRequest, tx, |f| {
        rpc::HeartbeatRequest::create(f, &Default::default()).as_union_value()
    })
}

pub fn subscribe() -> Vec<u8> {
    subscribe_config(100, 25, true)
}
pub fn subscribe_config(telemetry_ms: u16, bone_ms: u16, bones_enabled: bool) -> Vec<u8> {
    let mut f = fb::FlatBufferBuilder::new();
    let trackers = df::tracker::TrackerDataMask::create(
        &mut f,
        &df::tracker::TrackerDataMaskArgs {
            info: true,
            status: true,
            rotation: true,
            position: true,
            raw_acceleration: true,
            linear_acceleration: true,
            rotation_reference_adjusted: true,
            rotation_identity_adjusted: true,
            temp: true,
            tps: true,
            raw_magnetic_vector: true,
            stay_aligned: true,
            ..Default::default()
        },
    );
    let devices = df::device_data::DeviceDataMask::create(
        &mut f,
        &df::device_data::DeviceDataMaskArgs {
            device_data: true,
            tracker_data: Some(trackers),
        },
    );
    let config = df::DataFeedConfig::create(
        &mut f,
        &df::DataFeedConfigArgs {
            minimum_time_since_last: telemetry_ms.clamp(1, 5000),
            data_mask: Some(devices),
            bone_mask: false,
            server_guards_mask: true,
            synthetic_trackers_mask: Some(trackers),
            stay_aligned_pose_mask: true,
        },
    );
    let mut configurations = vec![config];
    if bones_enabled {
        configurations.push(df::DataFeedConfig::create(
            &mut f,
            &df::DataFeedConfigArgs {
                minimum_time_since_last: bone_ms.clamp(1, 5000),
                bone_mask: true,
                ..Default::default()
            },
        ));
    }
    let configs = f.create_vector(&configurations);
    let start = df::StartDataFeed::create(
        &mut f,
        &df::StartDataFeedArgs {
            data_feeds: Some(configs),
        },
    );
    let header = df::DataFeedMessageHeader::create(
        &mut f,
        &df::DataFeedMessageHeaderArgs {
            message_type: df::DataFeedMessage::StartDataFeed,
            message: Some(start.as_union_value()),
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

pub fn decode(bytes: &[u8]) -> Result<Vec<Update>, String> {
    let bundle = fb::root::<sx::MessageBundle<'_>>(bytes)
        .map_err(|e| format!("Invalid SolarXR frame: {e}"))?;
    decode_verified(bundle)
}
pub fn decode_verified(bundle: sx::MessageBundle) -> Result<Vec<Update>, String> {
    let mut updates = Vec::new();
    if let Some(headers) = bundle.data_feed_msgs() {
        for header in headers {
            if let Some(feed) = header.message_as_data_feed_update() {
                if feed.index() > 1 {
                    continue;
                }
                let mut out = Feed::default();
                if let Some(bones) = feed.bones() {
                    for bone in bones {
                        let (Some(h), Some(q)) = (bone.head_position_g(), bone.rotation_g()) else {
                            continue;
                        };
                        let length = bone.bone_length();
                        let head = [h.x(), h.y(), h.z()];
                        // q * (0, -length, 0); same local bone direction as the server.
                        let tail = [
                            head[0] - 2.0 * length * (q.x() * q.y() - q.w() * q.z()),
                            head[1] - length * (1.0 - 2.0 * (q.x() * q.x() + q.z() * q.z())),
                            head[2] - 2.0 * length * (q.y() * q.z() + q.w() * q.x()),
                        ];
                        if (0.0..=5.0).contains(&length)
                            && head
                                .iter()
                                .chain(&tail)
                                .all(|v| v.is_finite() && v.abs() < 1000.0)
                        {
                            out.bones.push(Bone {
                                body: bone.body_part().0,
                                head,
                                tail,
                            });
                        }
                    }
                }
                if feed.index() == 1 {
                    updates.push(Update::Bones(out.bones));
                    continue;
                }
                if let Some(guards) = feed.server_guards() {
                    out.can_yaw = guards.canDoYawReset();
                    out.can_mount = guards.canDoMounting();
                    out.can_height = guards.canDoUserHeightCalibration();
                }
                if let Some(devices) = feed.devices() {
                    for device in devices {
                        let hardware = device
                            .hardware_info()
                            .and_then(|h| h.hardware_identifier())
                            .unwrap_or_default()
                            .to_owned();
                        let battery = device
                            .hardware_status()
                            .and_then(|s| s.battery_pct_estimate());
                        let ping = device.hardware_status().and_then(|s| s.ping());
                        if let Some(trackers) = device.trackers() {
                            for tracker in trackers {
                                let Some(id) = tracker.tracker_id() else {
                                    continue;
                                };
                                let Some(device_id) = id.device_id() else {
                                    continue;
                                };
                                let info = tracker.info();
                                out.trackers.push(Tracker {
                                    key: TrackerKey {
                                        device: device_id.id(),
                                        sensor: id.tracker_num(),
                                    },
                                    name: info
                                        .and_then(|i| i.custom_name().or(i.display_name()))
                                        .unwrap_or("Tracker")
                                        .to_owned(),
                                    custom_name: info
                                        .and_then(|i| i.custom_name())
                                        .map(str::to_owned),
                                    body: info.map(|i| i.body_part().0).unwrap_or(0),
                                    status: tracker.status().0,
                                    hardware: hardware.clone(),
                                    battery,
                                    battery_runtime_us: device
                                        .hardware_status()
                                        .and_then(|h| h.battery_runtime_estimate())
                                        .and_then(|us| u64::try_from(us).ok()),
                                    ping,
                                    firmware: device
                                        .hardware_info()
                                        .and_then(|h| h.firmware_version())
                                        .unwrap_or("")
                                        .into(),
                                    firmware_date: device
                                        .hardware_info()
                                        .and_then(|h| h.firmware_date())
                                        .unwrap_or("")
                                        .into(),
                                    manufacturer: device
                                        .hardware_info()
                                        .and_then(|h| h.manufacturer())
                                        .unwrap_or("")
                                        .into(),
                                    display_name: info
                                        .and_then(|i| i.display_name())
                                        .unwrap_or("")
                                        .into(),
                                    address: device
                                        .hardware_info()
                                        .and_then(|h| h.ip_address())
                                        .map(|ip| {
                                            format!("udp://{}", std::net::Ipv4Addr::from(ip.addr()))
                                        }),
                                    board_name: device
                                        .hardware_info()
                                        .and_then(|h| h.board_type())
                                        .unwrap_or("")
                                        .into(),
                                    network_version: device
                                        .hardware_info()
                                        .and_then(|h| h.network_protocol_version()),
                                    board: device
                                        .hardware_info()
                                        .map(|h| h.official_board_type().0)
                                        .unwrap_or(0),
                                    rssi: device.hardware_status().and_then(|h| h.rssi()),
                                    voltage: device
                                        .hardware_status()
                                        .and_then(|h| h.battery_voltage()),
                                    temperature: tracker.temp().map(|t| t.temp()),
                                    packet_loss: device
                                        .hardware_status()
                                        .and_then(|h| h.packet_loss()),
                                    tps: tracker.tps(),
                                    rotation: tracker
                                        .rotation_identity_adjusted()
                                        .map(|q| [q.x(), q.y(), q.z(), q.w()]),
                                    raw_rotation: tracker
                                        .rotation()
                                        .map(|q| [q.x(), q.y(), q.z(), q.w()]),
                                    position: tracker.position().map(|p| [p.x(), p.y(), p.z()]),
                                    acceleration: tracker
                                        .raw_acceleration()
                                        .map(|p| [p.x(), p.y(), p.z()]),
                                    linear_acceleration: tracker
                                        .linear_acceleration()
                                        .map(|p| [p.x(), p.y(), p.z()]),
                                    magnetic: tracker
                                        .raw_magnetic_vector()
                                        .map(|p| [p.x(), p.y(), p.z()]),
                                    mounting: info
                                        .and_then(|i| i.mounting_orientation())
                                        .map(|q| [q.x(), q.y(), q.z(), q.w()]),
                                    imu: info
                                        .and_then(|i| i.imu_type().variant_name())
                                        .unwrap_or("Other")
                                        .into(),
                                    is_imu: info.is_some_and(|i| i.is_imu()),
                                    editable: info.is_some_and(|i| i.editable()),
                                    computed: info.is_some_and(|i| i.is_computed()),
                                    magnetometer: info.map(|i| i.magnetometer().0).unwrap_or(0),
                                    yaw_correction: tracker
                                        .stay_aligned()
                                        .map(|s| s.yaw_correction_in_deg()),
                                });
                            }
                        }
                    }
                }
                if let Some(trackers) = feed.synthetic_trackers() {
                    for tracker in trackers {
                        let Some(id) = tracker.tracker_id() else {
                            continue;
                        };
                        let info = tracker.info();
                        out.trackers.push(Tracker {
                            key: TrackerKey {
                                device: 0,
                                sensor: id.tracker_num().saturating_add(128),
                            },
                            name: info
                                .and_then(|i| i.display_name())
                                .unwrap_or("Virtual tracker")
                                .into(),
                            body: info.map(|i| i.body_part().0).unwrap_or(0),
                            status: tracker.status().0,
                            computed: true,
                            rotation: tracker.rotation().map(|q| [q.x(), q.y(), q.z(), q.w()]),
                            position: tracker.position().map(|p| [p.x(), p.y(), p.z()]),
                            ..Default::default()
                        });
                    }
                }
                updates.push(Update::Feed(out));
            }
        }
    }
    if let Some(headers) = bundle.rpc_msgs() {
        for header in headers {
            if let Some(reset) = header.message_as_reset_response() {
                updates.push(Update::Reset(ResetProgress {
                    tx: header.tx_id().map(|t| t.id()).unwrap_or(0),
                    kind: reset.reset_type().0,
                    done: reset.status() == rpc::ResetStatus::FINISHED,
                    progress_ms: reset.progress(),
                    duration_ms: reset.duration(),
                }));
            } else if let Some(pause) = header.message_as_tracking_pause_state_response() {
                updates.push(Update::Paused(pause.trackingPaused()));
            } else if let Some(settings) = header.message_as_settings_response() {
                let mut s = Settings::default();
                if let Some(steam) = settings.steam_vr_trackers() {
                    for (enabled, role) in [
                        (steam.waist(), "HIP"),
                        (steam.chest(), "CHEST"),
                        (steam.left_foot(), "LEFT_FOOT"),
                        (steam.right_foot(), "RIGHT_FOOT"),
                        (steam.left_knee(), "LEFT_LOWER_LEG"),
                        (steam.right_knee(), "RIGHT_LOWER_LEG"),
                        (steam.left_elbow(), "LEFT_LOWER_ARM"),
                        (steam.right_elbow(), "RIGHT_LOWER_ARM"),
                        (steam.left_hand(), "LEFT_HAND"),
                        (steam.right_hand(), "RIGHT_HAND"),
                    ] {
                        if enabled {
                            s.steamvr_roles.push(role.into());
                        }
                    }
                }
                if let Some(filter) = settings.filtering() {
                    s.filter = filter.type_().variant_name().map(str::to_owned);
                    s.filter_amount = Some(filter.amount());
                }
                updates.push(Update::Settings(s));
            } else if let Some(state) = header.message_as_vrcconfig_state_change_response() {
                updates.push(Update::Vrchat(decode_vrchat(state)));
            } else if header.message_type() == rpc::RpcMessage::HeartbeatResponse {
                updates.push(Update::Heartbeat);
            }
        }
    }
    Ok(updates)
}

fn decode_vrchat(s: rpc::VRCConfigStateChangeResponse<'_>) -> Vrchat {
    let mut out = Vrchat {
        supported: s.is_supported(),
        rows: Vec::new(),
    };
    let (Some(v), Some(current), Some(r)) = (s.validity(), s.state(), s.recommended()) else {
        return out;
    };
    let muted: Vec<_> = s.muted().map(|m| m.iter().collect()).unwrap_or_default();
    let on_off = |value| {
        if value {
            "vrc_config-on"
        } else {
            "vrc_config-off"
        }
        .to_string()
    };
    let enum_label =
        |prefix: &str, name: Option<&str>| format!("{prefix}-{}", name.unwrap_or("UNKNOWN"));
    let items = [
        (
            "userHeightOk",
            "user_height",
            v.user_height_ok(),
            format!("{:.2} m", current.user_height()),
            format!("{:.2} m", r.user_height()),
        ),
        (
            "legacyModeOk",
            "legacy_mode",
            v.legacy_mode_ok(),
            on_off(current.legacy_mode()),
            on_off(r.legacy_mode()),
        ),
        (
            "shoulderTrackingOk",
            "disable_shoulder_tracking",
            v.shoulder_tracking_ok(),
            on_off(current.shoulder_tracking_disabled()),
            on_off(r.shoulder_tracking_disabled()),
        ),
        (
            "calibrationRangeOk",
            "calibration_range",
            v.calibration_range_ok(),
            format!("{:.2} m", current.calibration_range()),
            format!("{:.2} m", r.calibration_range()),
        ),
        (
            "calibrationVisualsOk",
            "calibration_visuals",
            v.calibration_visuals_ok(),
            on_off(current.calibration_visuals()),
            on_off(r.calibration_visuals()),
        ),
        (
            "trackerModelOk",
            "tracker_model",
            v.tracker_model_ok(),
            enum_label(
                "vrc_config-tracker_model",
                current.tracker_model().variant_name(),
            ),
            enum_label("vrc_config-tracker_model", r.tracker_model().variant_name()),
        ),
        (
            "spineModeOk",
            "spine_mode",
            v.spine_mode_ok(),
            enum_label("vrc_config-spine_mode", current.spine_mode().variant_name()),
            r.spine_mode()
                .map(|m| {
                    m.iter()
                        .map(|m| enum_label("vrc_config-spine_mode", m.variant_name()))
                        .collect::<Vec<_>>()
                        .join(" / ")
                })
                .unwrap_or_default(),
        ),
        (
            "avatarMeasurementTypeOk",
            "avatar_measurement_type",
            v.avatar_measurement_type_ok(),
            enum_label(
                "vrc_config-avatar_measurement_type",
                current.avatar_measurement_type().variant_name(),
            ),
            enum_label(
                "vrc_config-avatar_measurement_type",
                r.avatar_measurement_type().variant_name(),
            ),
        ),
        (
            "shoulderWidthCompensationOk",
            "shoulder_width_compensation",
            v.shoulder_width_compensation_ok(),
            on_off(current.shoulder_width_compensation()),
            on_off(r.shoulder_width_compensation()),
        ),
    ];
    for (key, label, valid, current, recommended) in items {
        out.rows.push(VrchatRow {
            key: key.into(),
            label: format!("vrc_config-{label}"),
            valid,
            muted: muted.contains(&key),
            current,
            recommended,
        });
    }
    out
}

/// Binary file payloads stay bytes; never materialize a JSON value for every byte.
pub fn files(bytes: &[u8]) -> Result<Vec<std::sync::Arc<[u8]>>, String> {
    let bundle = fb::root::<sx::MessageBundle>(bytes).map_err(|e| e.to_string())?;
    files_verified(bundle)
}
pub fn files_verified(bundle: sx::MessageBundle) -> Result<Vec<std::sync::Arc<[u8]>>, String> {
    let mut files = Vec::new();
    if let Some(headers) = bundle.rpc_msgs() {
        for header in headers {
            if let Some(file) = header.message_as_save_file_notification() {
                files.push(
                    file.data()
                        .map(|b| std::sync::Arc::from(b.bytes()))
                        .unwrap_or_default(),
                );
            }
        }
    }
    Ok(files)
}
