//! SlimeVR's vrconfig.yml is the persistent source. Runtime structs are an adapter,
//! not a second configuration file. Keep unmapped YAML fields when saving.
mod persistence;
mod worker;
use crate::{api::FrontendConfig, receiver::normalize_mac};
pub use persistence::save;
pub(crate) use persistence::save_measured;
use serde::{de::DeserializeOwned, Serialize};
use serde_yaml_ng::{Mapping, Value};
use slimevr_core::{
    calibration::ArmsResetMode, pose::TrackerBinding, skeleton::BodyPosition as B, Quaternion as Q,
};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};
pub(crate) use worker::{Completion, Writer};

const MAX_CONFIG_BYTES: u64 = 8 * 1024 * 1024;
const VERSION: u64 = 15;
type Fields = &'static [(&'static str, &'static str)];
const OFFSETS: Fields = &[
    ("headShift", "head_shift"),
    ("neckLength", "neck_length"),
    ("upperChestLength", "upper_chest_length"),
    ("chestLength", "chest_length"),
    ("waistLength", "waist_length"),
    ("hipLength", "hip_length"),
    ("hipsWidth", "hips_width"),
    ("upperLegLength", "upper_leg_length"),
    ("lowerLegLength", "lower_leg_length"),
    ("footLength", "foot_length"),
    ("footShift", "foot_shift"),
    ("chestOffset", "chest_offset"),
    ("hipOffset", "hip_offset"),
    ("skeletonOffset", "skeleton_offset"),
    ("shouldersDistance", "shoulders_distance"),
    ("shouldersWidth", "shoulders_width"),
    ("upperArmLength", "upper_arm_length"),
    ("lowerArmLength", "lower_arm_length"),
    ("handDistanceY", "hand_y"),
    ("handDistanceZ", "hand_z"),
    ("elbowOffset", "elbow_offset"),
];
const TOGGLES: Fields = &[
    ("extendedSpine", "extended_spine"),
    ("extendedPelvis", "extended_pelvis"),
    ("extendedKnee", "extended_knee"),
    ("forceArmsFromHMD", "force_arms_from_hmd"),
    ("enforceConstraints", "enforce_constraints"),
    ("usePosition", "use_position"),
    ("correctConstraints", "correct_constraints"),
];
const RATIOS: Fields = &[
    ("waistFromChestHipAveraging", "waist_from_chest_hip"),
    ("waistFromChestLegsAveraging", "waist_from_chest_legs"),
    ("hipFromChestLegsAveraging", "hip_from_chest_legs"),
    ("hipFromWaistLegsAveraging", "hip_from_waist_legs"),
    ("hipLegsAveraging", "hip_legs"),
    ("kneeTrackerAnkleAveraging", "knee_tracker_ankle"),
    ("kneeAnkleAveraging", "knee_ankle"),
];
const LEG_TOGGLES: Fields = &[
    ("floorClip", "floor_clip"),
    ("skatingCorrection", "skating"),
    ("toeSnap", "toe_snap"),
    ("footPlant", "foot_plant"),
];
const LEG_VALUES: Fields = &[
    ("correctionStrength", "correction_strength"),
    ("alwaysUseFloorclip", "always_use_floor_clip"),
];
const RELAXED: Fields = &[
    ("enabled", "enabled"),
    ("upperLegAngleInDeg", "upper_leg_degrees"),
    ("lowerLegAngleInDeg", "lower_leg_degrees"),
    ("footAngleInDeg", "foot_degrees"),
];
const AUTO: Fields = &[
    ("numEpochs", "epochs"),
    ("cursorIncrement", "cursor_increment"),
    ("minDataDistance", "min_distance"),
    ("maxDataDistance", "max_distance"),
    ("initialAdjustRate", "initial_adjust_rate"),
    ("adjustRateDecay", "adjust_rate_decay"),
    ("randomizeFrameOrder", "randomize"),
    ("scaleEachStep", "scale_each_step"),
    ("useFrameFiltering", "filter_outliers"),
    ("calcInitError", "calc_initial_error"),
    ("useSkeletonHeight", "use_skeleton_height"),
    ("slideErrorFactor", "slide_factor"),
    ("offsetSlideErrorFactor", "offset_slide_factor"),
    ("footHeightOffsetErrorFactor", "foot_height_factor"),
    ("bodyProportionErrorFactor", "proportion_factor"),
    ("heightErrorFactor", "height_factor"),
    ("positionErrorFactor", "position_factor"),
    ("positionOffsetErrorFactor", "position_offset_factor"),
    ("maxFinalError", "max_final_error"),
];
const TAP: Fields = &[
    ("setupMode", "setup_mode"),
    ("yawResetEnabled", "yaw_enabled"),
    ("fullResetEnabled", "full_enabled"),
    ("mountingResetEnabled", "mounting_enabled"),
    ("yawResetTaps", "yaw_taps"),
    ("fullResetTaps", "full_taps"),
    ("mountingResetTaps", "mounting_taps"),
    ("numberTrackersOverThreshold", "max_moving"),
];
const EXTRAS: Fields = &[
    ("imuTypes", "imu_types"),
    ("magnetometers", "magnetometers"),
    ("flexResistance", "flex_resistance"),
    ("flexAngles", "flex_angles"),
    ("savedMountingResets", "saved_mounting_resets"),
];

fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
fn get<'a>(root: &'a Value, path: &[&str]) -> &'a Value {
    let mut node = root;
    for key in path {
        node = &node[*key];
    }
    node
}
pub(crate) fn put(root: &mut Value, path: &[&str], value: impl Serialize) -> io::Result<()> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| error("empty configuration path"))?;
    let mut node = root;
    for key in parents {
        if node.is_null() {
            *node = Value::Mapping(Mapping::new());
        }
        let map = node.as_mapping_mut().ok_or_else(|| {
            error(format!(
                "configuration section {} must be a map",
                path.join(".")
            ))
        })?;
        node = map
            .entry(Value::String((*key).into()))
            .or_insert(Value::Mapping(Mapping::new()));
    }
    if node.is_null() {
        *node = Value::Mapping(Mapping::new());
    }
    let map = node.as_mapping_mut().ok_or_else(|| {
        error(format!(
            "configuration section {} must be a map",
            path.join(".")
        ))
    })?;
    map.insert(
        Value::String((*last).into()),
        serde_yaml_ng::to_value(value).map_err(|e| error(e.to_string()))?,
    );
    Ok(())
}
fn read<T: DeserializeOwned>(root: &Value, path: &[&str], default: T) -> io::Result<T> {
    let value = get(root, path);
    if value.is_null() {
        return Ok(default);
    }
    serde_yaml_ng::from_value(value.clone()).map_err(|e| error(format!("{}: {e}", path.join("."))))
}
fn fields<T: Serialize + DeserializeOwned>(
    root: &Value,
    path: &[&str],
    default: T,
    fields: Fields,
) -> io::Result<T> {
    let mut object = serde_json::to_value(default).map_err(|e| error(e.to_string()))?;
    let node = get(root, path);
    if !node.is_null() && !node.is_mapping() {
        return Err(error(format!("{} must be a map", path.join("."))));
    }
    for (yaml, rust) in fields {
        if !node[*yaml].is_null() {
            object[*rust] = serde_yaml_ng::from_value(node[*yaml].clone())
                .map_err(|e| error(format!("{}.{yaml}: {e}", path.join("."))))?;
        }
    }
    serde_json::from_value(object).map_err(|e| error(format!("{}: {e}", path.join("."))))
}
fn write_fields<T: Serialize>(
    root: &mut Value,
    path: &[&str],
    value: &T,
    fields: Fields,
) -> io::Result<()> {
    let object = serde_json::to_value(value).map_err(|e| error(e.to_string()))?;
    for (yaml, rust) in fields {
        let mut path = path.to_vec();
        path.push(yaml);
        put(root, &path, &object[*rust])?;
    }
    Ok(())
}
fn seconds(root: &Value, path: &[&str], default: u64) -> io::Result<u64> {
    let seconds: f32 = read(root, path, default as f32 / 1000.0)?;
    if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
        return Err(error(format!("{} must be 0..60 seconds", path.join("."))));
    }
    Ok((seconds * 1000.0).round() as u64)
}
pub(crate) fn body(value: &str) -> io::Result<B> {
    serde_json::from_value(serde_json::Value::String(
        value
            .strip_prefix("body:")
            .unwrap_or(value)
            .to_ascii_lowercase(),
    ))
    .map_err(|_| error(format!("unknown tracker body position {value}")))
}
pub(crate) fn body_name(value: B) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
fn tracker_id(name: &str) -> Option<(String, u8)> {
    if let Some(udp) = name.strip_prefix("udp://") {
        let (mac, sensor) = udp.rsplit_once('/')?;
        return Some((normalize_mac(mac).ok()?, sensor.parse().ok()?));
    }
    let (address, sensor) = name.rsplit_once('/')?;
    if address.len() != 12 || !address.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        format!("hid:{}", address.to_ascii_uppercase()),
        sensor.parse().ok()?,
    ))
}
fn tracker_name(key: &str, sensor: u8) -> String {
    if let Some(address) = key.strip_prefix("hid:") {
        format!("{address}/{sensor}")
    } else {
        format!("udp://{key}/{sensor}")
    }
}
fn named_id(id: &str) -> String {
    if let Some(id) = id.strip_prefix("hid:") {
        id.into()
    } else {
        format!("udp://{id}")
    }
}

fn mounting(body: B) -> Q {
    use B::*;
    match body {
        LeftLowerArm
        | LeftHand
        | LeftIndexProximal
        | LeftIndexIntermediate
        | LeftIndexDistal
        | LeftMiddleProximal
        | LeftMiddleIntermediate
        | LeftMiddleDistal
        | LeftRingProximal
        | LeftRingIntermediate
        | LeftRingDistal
        | LeftLittleProximal
        | LeftLittleIntermediate
        | LeftLittleDistal => Q::new(0.707, 0.0, 0.707, 0.0),
        RightLowerArm
        | RightHand
        | RightIndexProximal
        | RightIndexIntermediate
        | RightIndexDistal
        | RightMiddleProximal
        | RightMiddleIntermediate
        | RightMiddleDistal
        | RightRingProximal
        | RightRingIntermediate
        | RightRingDistal
        | RightLittleProximal
        | RightLittleIntermediate
        | RightLittleDistal => Q::new(0.707, 0.0, -0.707, 0.0),
        LeftUpperArm | LeftLowerLeg => Q::new(0.383, 0.0, 0.924, 0.0),
        RightUpperArm | RightLowerLeg => Q::new(0.383, 0.0, -0.924, 0.0),
        _ => Q::new(0.0, 0.0, 1.0, 0.0),
    }
}

pub fn load(path: &Path) -> io::Result<FrontendConfig> {
    if fs::metadata(path)?.len() > MAX_CONFIG_BYTES {
        return Err(error("configuration file exceeds 8 MiB"));
    }
    let bytes = fs::read(path)?;
    let mut root: Value =
        serde_yaml_ng::from_slice(&bytes).map_err(|e| error(format!("{}: {e}", path.display())))?;
    // Read previous development state once. All subsequent writes are YAML.
    if !root["pose"].is_null() && root["version"].as_u64() == Some(1) {
        let config: FrontendConfig =
            serde_yaml_ng::from_value(root).map_err(|e| error(e.to_string()))?;
        config.validate().map_err(error)?;
        return Ok(config);
    }
    migrate(&mut root)?;
    from_yaml(root)
}
pub fn from_yaml(root: Value) -> io::Result<FrontendConfig> {
    if !root.is_mapping() {
        return Err(error("SlimeVR configuration must be a YAML map"));
    }
    let mut c = FrontendConfig {
        sample_count: 1500,
        ..Default::default()
    };
    let p = &mut c.pose;
    *p = fields(&root, &["rust", "pose"], p.clone(), EXTRAS)?;
    p.filter = fields(
        &root,
        &["filters"],
        p.filter,
        &[("type", "mode"), ("amount", "amount")],
    )?;
    c.ignored_steps = read(
        &root,
        &["trackingChecklist", "ignoredStepsIds"],
        Default::default(),
    )?;
    p.skeleton = fields(&root, &["skeleton", "offsets"], p.skeleton, OFFSETS)?;
    p.skeleton = fields(&root, &["skeleton", "toggles"], p.skeleton, TOGGLES)?;
    p.skeleton = fields(&root, &["skeleton", "values"], p.skeleton, RATIOS)?;
    p.legs = fields(&root, &["skeleton", "toggles"], p.legs, LEG_TOGGLES)?;
    p.legs = fields(&root, &["legTweaks"], p.legs, LEG_VALUES)?;
    p.legs.enabled = read(&root, &["rust", "pose", "legsEnabled"], p.legs.enabled)?;
    p.localizer.enabled = read(&root, &["skeleton", "toggles", "selfLocalization"], false)?;
    p.send_derived_velocity = read(&root, &["velocityConfig", "sendDerivedVelocity"], false)?;
    p.yaw_reset_smooth_seconds = read(&root, &["resetsConfig", "yawResetSmoothTime"], 0.0)?;
    p.full_reset_delay_ms = seconds(&root, &["resetsConfig", "fullResetDelay"], 3000)?;
    p.mounting_reset_delay_ms = seconds(&root, &["resetsConfig", "mountingResetDelay"], 3000)?;
    p.arms_reset_mode = match read::<String>(&root, &["resetsConfig", "mode"], "BACK".into())?
        .to_ascii_uppercase()
        .as_str()
    {
        "BACK" => ArmsResetMode::Back,
        "FORWARD" => ArmsResetMode::Forward,
        "TPOSE_UP" => ArmsResetMode::TposeUp,
        "TPOSE_DOWN" => ArmsResetMode::TposeDown,
        _ => return Err(error("unknown resetsConfig.mode")),
    };
    p.save_mounting_reset = read(&root, &["resetsConfig", "saveMountingReset"], false)?;
    p.reset_hmd_pitch = read(&root, &["resetsConfig", "resetHmdPitch"], false)?;
    p.reset_mounting_feet = read(&root, &["resetsConfig", "resetMountingFeet"], false)?;
    p.alignment.enabled = read(&root, &["stayAlignedConfig", "enabled"], false)?;
    p.alignment.standing = fields(
        &root,
        &["stayAlignedConfig", "standingRelaxedPose"],
        p.alignment.standing,
        RELAXED,
    )?;
    p.alignment.sitting = fields(
        &root,
        &["stayAlignedConfig", "sittingRelaxedPose"],
        p.alignment.sitting,
        RELAXED,
    )?;
    p.alignment.flat = fields(
        &root,
        &["stayAlignedConfig", "flatRelaxedPose"],
        p.alignment.flat,
        RELAXED,
    )?;
    let hmd: f32 = read(&root, &["skeleton", "hmdHeight"], 0.0)?;
    let floor: f32 = read(&root, &["skeleton", "floorHeight"], 0.0)?;
    if !hmd.is_finite() || !floor.is_finite() {
        return Err(error("HMD and floor heights must be finite"));
    }
    p.hmd_height = (hmd > 0.0).then_some(hmd - floor);
    p.taps = fields(&root, &["tapDetection"], p.taps, TAP)?;
    for (key, target, default) in [
        ("yawResetTracker", &mut p.taps.yaw_tracker, B::Chest),
        (
            "fullResetTracker",
            &mut p.taps.full_tracker,
            B::LeftUpperLeg,
        ),
        (
            "mountingResetTracker",
            &mut p.taps.mounting_tracker,
            B::RightUpperLeg,
        ),
    ] {
        let section = get(&root, &["tapDetection"]);
        *target = if section
            .as_mapping()
            .is_some_and(|m| m.contains_key(Value::String(key.into())))
        {
            if section[key].is_null() {
                None
            } else {
                Some(body(section[key].as_str().ok_or_else(|| {
                    error("tap reset tracker must be a body name")
                })?)?)
            }
        } else {
            Some(default)
        };
    }
    p.taps.yaw_delay_ms = seconds(&root, &["tapDetection", "yawResetDelay"], 200)?;
    p.taps.full_delay_ms = seconds(&root, &["tapDetection", "fullResetDelay"], 1000)?;
    p.taps.mounting_delay_ms = seconds(&root, &["tapDetection", "mountingResetDelay"], 1000)?;
    p.taps.enabled = read(
        &root,
        &["rust", "pose", "tapEnabled"],
        p.taps.yaw_enabled || p.taps.full_enabled || p.taps.mounting_enabled,
    )?;
    c.auto_bone = fields(&root, &["autoBone"], c.auto_bone, AUTO)?;
    let seed: i64 = read(&root, &["autoBone", "randSeed"], 4)?;
    c.auto_bone.seed = seed as u64;
    c.magnetometers_enabled = read(&root, &["server", "useMagnetometerOnAllTrackers"], false)?;
    c.save_recordings = read(&root, &["autoBone", "saveRecordings"], c.save_recordings)?;
    c.sample_count = read(&root, &["autoBone", "sampleCount"], c.sample_count)?;
    c.sample_ms = read(&root, &["autoBone", "sampleRateMs"], c.sample_ms)?;
    c.tracker_port = read(&root, &["server", "trackerPort"], 6969)?;
    c.osc = crate::osc::Settings::from_yaml(&root).map_err(error)?;
    c.steam_vr = read(&root, &["bridges", "steamvr"], Default::default())?;
    for (_, role, _) in crate::steamvr::ROLES {
        c.steam_vr.trackers.entry(role.into()).or_insert(false);
    }
    c.device_ids = read(&root, &["rust", "deviceIds"], Default::default())?;
    c.pose.bindings = read(&root, &["rust", "otherBindings"], Default::default())?;
    c.tracker_names = read(&root, &["rust", "otherTrackerNames"], Default::default())?;
    let known: Vec<String> = read(&root, &["knownDevices"], Vec::new())?;
    for mac in known {
        c.allowed_macs.push(normalize_mac(&mac).map_err(error)?);
    }
    c.allowed_macs.sort();
    c.allowed_macs.dedup();
    let trackers = get(&root, &["trackers"]);
    if !trackers.is_null() && !trackers.is_mapping() {
        return Err(error("trackers must be a map"));
    }
    if let Some(trackers) = trackers.as_mapping() {
        for (name, tracker) in trackers {
            let Some((mac, sensor_id)) = name.as_str().and_then(tracker_id) else {
                continue;
            };
            if !tracker.is_mapping() {
                return Err(error("UDP tracker settings must be a map"));
            }
            let id = format!("{mac}/{sensor_id}");
            if !tracker["shouldHaveMagEnabled"].is_null() {
                c.mag_preferences
                    .insert(id.clone(), read(tracker, &["shouldHaveMagEnabled"], true)?);
            }
            if let Some(name) = tracker["customName"].as_str() {
                c.tracker_names.insert(id.clone(), name.into());
            }
            let Some(designation) = tracker["designation"].as_str() else {
                continue;
            };
            let Ok(body) = body(designation) else {
                continue;
            };
            let mounting: Q = read(tracker, &["mountingOrientation"], mounting(body))?;
            c.pose.bindings.push(TrackerBinding {
                device_key: mac,
                sensor_id,
                body,
                mounting,
            });
            if !tracker["mountingResetOrientation"].is_null() {
                c.pose.saved_mounting_resets.insert(
                    body,
                    read(tracker, &["mountingResetOrientation"], Q::IDENTITY)?,
                );
            }
        }
    }
    c.validate().map_err(error)?;
    c.yaml = root;
    Ok(c)
}

pub fn to_yaml(c: &FrontendConfig) -> io::Result<Value> {
    c.validate().map_err(error)?;
    let mut root = c.yaml.clone();
    if root.is_null() {
        root = Value::Mapping(Mapping::new());
    }
    let p = &c.pose;
    put(
        &mut root,
        &["resetsConfig", "resetHmdPitch"],
        p.reset_hmd_pitch,
    )?;
    put(
        &mut root,
        &["resetsConfig", "resetMountingFeet"],
        p.reset_mounting_feet,
    )?;
    put(
        &mut root,
        &["velocityConfig", "sendDerivedVelocity"],
        p.send_derived_velocity,
    )?;
    put(&mut root, &["version"], VERSION.to_string())?;
    put(&mut root, &["server", "trackerPort"], c.tracker_port)?;
    put(
        &mut root,
        &["server", "useMagnetometerOnAllTrackers"],
        c.magnetometers_enabled,
    )?;
    write_fields(
        &mut root,
        &["filters"],
        &p.filter,
        &[("type", "mode"), ("amount", "amount")],
    )?;
    write_fields(&mut root, &["skeleton", "offsets"], &p.skeleton, OFFSETS)?;
    write_fields(&mut root, &["skeleton", "toggles"], &p.skeleton, TOGGLES)?;
    write_fields(&mut root, &["skeleton", "values"], &p.skeleton, RATIOS)?;
    write_fields(&mut root, &["skeleton", "toggles"], &p.legs, LEG_TOGGLES)?;
    put(
        &mut root,
        &["skeleton", "toggles", "selfLocalization"],
        p.localizer.enabled,
    )?;
    write_fields(&mut root, &["legTweaks"], &p.legs, LEG_VALUES)?;
    put(
        &mut root,
        &["bridges", "steamvr", "automaticSharedTrackersToggling"],
        c.steam_vr.automatic,
    )?;
    for (role, enabled) in &c.steam_vr.trackers {
        put(
            &mut root,
            &["bridges", "steamvr", "trackers", role],
            enabled,
        )?;
    }
    for (_, role, _) in crate::steamvr::ROLES {
        put(
            &mut root,
            &["bridges", "steamvr", "trackers", role],
            c.steam_vr.enabled(role),
        )?;
    }
    let floor: f32 = read(&root, &["skeleton", "floorHeight"], 0.0)?;
    if p.hmd_height.is_none() {
        put(&mut root, &["skeleton", "floorHeight"], 0.0)?;
    }
    put(
        &mut root,
        &["skeleton", "hmdHeight"],
        p.hmd_height.map(|h| h + floor).unwrap_or(0.0),
    )?;
    put(
        &mut root,
        &["resetsConfig", "yawResetSmoothTime"],
        p.yaw_reset_smooth_seconds,
    )?;
    put(
        &mut root,
        &["resetsConfig", "fullResetDelay"],
        p.full_reset_delay_ms as f32 / 1000.0,
    )?;
    put(
        &mut root,
        &["resetsConfig", "mountingResetDelay"],
        p.mounting_reset_delay_ms as f32 / 1000.0,
    )?;
    put(
        &mut root,
        &["resetsConfig", "mode"],
        match p.arms_reset_mode {
            ArmsResetMode::Back => "BACK",
            ArmsResetMode::Forward => "FORWARD",
            ArmsResetMode::TposeUp => "TPOSE_UP",
            ArmsResetMode::TposeDown => "TPOSE_DOWN",
        },
    )?;
    put(
        &mut root,
        &["resetsConfig", "saveMountingReset"],
        p.save_mounting_reset,
    )?;
    put(
        &mut root,
        &["stayAlignedConfig", "enabled"],
        p.alignment.enabled,
    )?;
    write_fields(
        &mut root,
        &["stayAlignedConfig", "standingRelaxedPose"],
        &p.alignment.standing,
        RELAXED,
    )?;
    write_fields(
        &mut root,
        &["stayAlignedConfig", "sittingRelaxedPose"],
        &p.alignment.sitting,
        RELAXED,
    )?;
    write_fields(
        &mut root,
        &["stayAlignedConfig", "flatRelaxedPose"],
        &p.alignment.flat,
        RELAXED,
    )?;
    write_fields(&mut root, &["tapDetection"], &p.taps, TAP)?;
    for (key, target) in [
        ("yawResetTracker", p.taps.yaw_tracker),
        ("fullResetTracker", p.taps.full_tracker),
        ("mountingResetTracker", p.taps.mounting_tracker),
    ] {
        put(
            &mut root,
            &["tapDetection", key],
            target.map(|b| body_name(b).to_ascii_uppercase()),
        )?;
    }
    for (key, ms) in [
        ("yawResetDelay", p.taps.yaw_delay_ms),
        ("fullResetDelay", p.taps.full_delay_ms),
        ("mountingResetDelay", p.taps.mounting_delay_ms),
    ] {
        put(&mut root, &["tapDetection", key], ms as f32 / 1000.0)?;
    }
    write_fields(&mut root, &["autoBone"], &c.auto_bone, AUTO)?;
    put(
        &mut root,
        &["autoBone", "randSeed"],
        c.auto_bone.seed as i64,
    )?;
    put(
        &mut root,
        &["autoBone", "saveRecordings"],
        c.save_recordings,
    )?;
    put(&mut root, &["autoBone", "sampleCount"], c.sample_count)?;
    put(&mut root, &["autoBone", "sampleRateMs"], c.sample_ms)?;
    put(&mut root, &["knownDevices"], &c.allowed_macs)?;
    put(&mut root, &["rust", "deviceIds"], &c.device_ids)?;
    let other_bindings: Vec<_> = p
        .bindings
        .iter()
        .filter(|b| tracker_id(&tracker_name(&b.device_key, b.sensor_id)).is_none())
        .collect();
    put(&mut root, &["rust", "otherBindings"], other_bindings)?;
    let other_names: std::collections::BTreeMap<_, _> = c
        .tracker_names
        .iter()
        .filter(|(id, _)| tracker_id(&named_id(id)).is_none())
        .collect();
    put(&mut root, &["rust", "otherTrackerNames"], other_names)?;
    write_fields(&mut root, &["rust", "pose"], p, EXTRAS)?;
    put(&mut root, &["rust", "pose", "legsEnabled"], p.legs.enabled)?;
    put(&mut root, &["rust", "pose", "tapEnabled"], p.taps.enabled)?;

    let mut names = BTreeSet::new();
    if let Some(map) = root["trackers"].as_mapping() {
        for name in map.keys().filter_map(Value::as_str) {
            if tracker_id(name).is_some() {
                names.insert(name.to_owned());
            }
        }
    }
    let mut candidates: Vec<_> = p
        .bindings
        .iter()
        .map(|b| tracker_name(&b.device_key, b.sensor_id))
        .collect();
    candidates.extend(c.tracker_names.keys().map(|id| named_id(id)));
    put(
        &mut root,
        &["trackingChecklist", "ignoredStepsIds"],
        &c.ignored_steps,
    )?;
    candidates.extend(c.mag_preferences.keys().map(|id| named_id(id)));
    for candidate in candidates {
        if let Some(id) = tracker_id(&candidate) {
            if !names
                .iter()
                .any(|name| tracker_id(name) == Some(id.clone()))
            {
                names.insert(candidate);
            }
        }
    }
    for name in names {
        let (mac, sensor) = tracker_id(&name).unwrap();
        let id = format!("{mac}/{sensor}");
        if let Some(enabled) = c.mag_preferences.get(&id) {
            put(
                &mut root,
                &["trackers", &name, "shouldHaveMagEnabled"],
                *enabled,
            )?;
        }
        let binding = p
            .bindings
            .iter()
            .find(|b| b.device_key == mac && b.sensor_id == sensor);
        if let Some(binding) = binding {
            put(
                &mut root,
                &["trackers", &name, "designation"],
                format!("body:{}", body_name(binding.body)),
            )?;
            put(
                &mut root,
                &["trackers", &name, "mountingOrientation"],
                binding.mounting,
            )?;
            // Keep saved orientation when saveMountingReset is off, just like Java.
            if let Some(q) = p.saved_mounting_resets.get(&binding.body) {
                put(
                    &mut root,
                    &["trackers", &name, "mountingResetOrientation"],
                    q,
                )?;
            }
        } else if root["trackers"][&name]["designation"]
            .as_str()
            .is_some_and(|d| body(d).is_ok())
        {
            put(
                &mut root,
                &["trackers", &name, "designation"],
                Option::<String>::None,
            )?;
        }
        if let Some(custom) = c.tracker_names.get(&id) {
            put(&mut root, &["trackers", &name, "customName"], custom)?;
        }
    }
    if let Some(sources) = c.yaml["rust"]["sourceNames"].as_mapping() {
        for (key, name) in sources {
            let (Some(key), Some(name)) = (key.as_str(), name.as_str()) else {
                continue;
            };
            let binding = p
                .bindings
                .iter()
                .find(|b| b.device_key == key && b.sensor_id == 0);
            put(
                &mut root,
                &["trackers", name, "designation"],
                binding.map(|b| format!("body:{}", body_name(b.body))),
            )?;
            if let Some(custom) = c.tracker_names.get(&format!("{key}/0")) {
                put(&mut root, &["trackers", name, "customName"], custom)?;
            }
        }
    }
    c.osc.write_yaml(&mut root)?;
    Ok(root)
}

pub fn forget_device(c: &mut FrontendConfig, mac: &str) {
    if let Some(trackers) = c.yaml["trackers"].as_mapping_mut() {
        trackers.retain(|key, _| {
            key.as_str()
                .and_then(tracker_id)
                .is_none_or(|(key, _)| key != mac)
        });
    }
}

pub fn default_path() -> PathBuf {
    let local = PathBuf::from("vrconfig.yml");
    if local.exists() || Path::new("config").is_dir() {
        return local;
    }
    if Path::new("vrconfig.yaml").exists() {
        return PathBuf::from("vrconfig.yaml");
    }
    #[cfg(target_os = "windows")]
    let directory = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let directory =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let directory = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let path = directory
        .map(|d| d.join("dev.slimevr.SlimeVR/vrconfig.yml"))
        .unwrap_or(local);
    if !path.exists() && path.with_extension("yaml").exists() {
        path.with_extension("yaml")
    } else {
        path
    }
}

// Mirrors CurrentVRConfigConverter's v1..v15 migrations. Unknown fields remain.
fn copy(root: &mut Value, old: &[&str], new: &[&str]) -> io::Result<()> {
    let value = get(root, old).clone();
    if !value.is_null() {
        put(root, new, value)?;
    }
    Ok(())
}
fn remove(root: &mut Value, path: &[&str]) {
    let (last, parents) = path.split_last().unwrap();
    let mut node = root;
    for key in parents {
        if !node.is_mapping() {
            return;
        }
        node = &mut node[*key];
    }
    if let Some(map) = node.as_mapping_mut() {
        map.remove(Value::String((*last).into()));
    }
}
fn equals(root: &Value, path: &[&str], value: f64) -> bool {
    get(root, path)
        .as_f64()
        .is_some_and(|v| (v - value).abs() < 1e-7)
}
pub fn migrate(root: &mut Value) -> io::Result<()> {
    if root.is_null() {
        *root = Value::Mapping(Mapping::new());
    }
    if !root.is_mapping() {
        return Err(error("SlimeVR configuration must be a YAML map"));
    }
    let version = if root["version"].is_null() {
        VERSION
    } else {
        root["version"]
            .as_u64()
            .or_else(|| root["version"].as_str()?.parse().ok())
            .ok_or_else(|| error("configuration version must be an integer"))?
    };
    if version == 0 || version > VERSION {
        return Err(error(format!(
            "unsupported SlimeVR configuration version {version}"
        )));
    }
    if version < 2 {
        if root["window"].is_mapping() {
            copy(root, &["zoom"], &["window", "zoom"])?;
            remove(root, &["zoom"]);
        }
        if let Some(trackers) = root["trackers"].as_sequence().cloned() {
            let mut map = Mapping::new();
            for mut tracker in trackers {
                let name = tracker["name"]
                    .as_str()
                    .ok_or_else(|| error("legacy tracker is missing its name"))?
                    .to_owned();
                remove(&mut tracker, &["name"]);
                map.insert(Value::String(name), tracker);
            }
            put(root, &["trackers"], map)?;
        }
        copy(root, &["bridge"], &["bridges"])?;
        remove(root, &["bridge"]);
        if let Some(body) = root["body"].as_mapping().cloned() {
            for (key, value) in body {
                if value.is_number() {
                    let key = key
                        .as_str()
                        .ok_or_else(|| error("body offset key must be a string"))?;
                    put(root, &["skeleton", "offsets", key], value)?;
                }
            }
            for (old, new) in [
                ("shoulersWidth", "shouldersWidth"),
                ("shoulersDistance", "shouldersDistance"),
            ] {
                copy(
                    root,
                    &["skeleton", "offsets", old],
                    &["skeleton", "offsets", new],
                )?;
                remove(root, &["skeleton", "offsets", old]);
            }
            remove(root, &["body"]);
        }
    }
    if version < 3
        && get(root, &["filters", "amount"])
            .as_f64()
            .is_some_and(|v| v > 2.0)
    {
        put(root, &["filters", "amount"], 0.2)?;
    }
    if version < 4 {
        let names: Vec<_> = root["trackers"]
            .as_mapping()
            .map(|m| {
                m.keys()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        for name in names {
            copy(
                root,
                &["trackers", &name, "mountingRotation"],
                &["trackers", &name, "mountingOrientation"],
            )?;
            remove(root, &["trackers", &name, "mountingRotation"]);
        }
    }
    if version < 5 && get(root, &["skeleton", "offsets"]).is_mapping() {
        let torso: f32 = read(root, &["skeleton", "offsets", "torsoLength"], 0.4)?;
        let chest: f32 = read(root, &["skeleton", "offsets", "chestDistance"], 0.16)?;
        let waist: f32 = read(root, &["skeleton", "offsets", "waistDistance"], 0.04)?;
        copy(
            root,
            &["skeleton", "offsets", "chestDistance"],
            &["skeleton", "offsets", "chestLength"],
        )?;
        copy(
            root,
            &["skeleton", "offsets", "waistDistance"],
            &["skeleton", "offsets", "hipLength"],
        )?;
        put(
            root,
            &["skeleton", "offsets", "waistLength"],
            torso - chest - waist,
        )?;
        let legs: f32 = read(root, &["skeleton", "offsets", "legsLength"], 0.92)?;
        let knee: f32 = read(root, &["skeleton", "offsets", "kneeHeight"], 0.5)?;
        put(
            root,
            &["skeleton", "offsets", "upperLegLength"],
            legs - knee,
        )?;
        put(root, &["skeleton", "offsets", "lowerLegLength"], knee)?;
        for key in [
            "torsoLength",
            "chestDistance",
            "waistDistance",
            "legsLength",
            "kneeHeight",
        ] {
            remove(root, &["skeleton", "offsets", key]);
        }
    }
    if version < 6 {
        for axis in ["Y", "Z"] {
            copy(
                root,
                &["skeleton", "offsets", &format!("controllerDistance{axis}")],
                &["skeleton", "offsets", &format!("handDistance{axis}")],
            )?;
        }
    }
    if version < 7 {
        for key in ["chestOffset", "hipOffset", "elbowOffset"] {
            let value = get(root, &["skeleton", "offsets", key]);
            if !value.is_null() {
                let v: f32 = read(root, &["skeleton", "offsets", key], 0.0)?;
                put(root, &["skeleton", "offsets", key], -v)?;
            }
        }
    }
    if version < 8 {
        for (old, new) in [
            ("resetBinding", "fullResetBinding"),
            ("quickResetBinding", "yawResetBinding"),
            ("resetMountingBinding", "mountingResetBinding"),
            ("resetDelay", "fullResetDelay"),
            ("quickResetDelay", "yawResetDelay"),
            ("resetMountingDelay", "mountingResetDelay"),
        ] {
            copy(root, &["keybindings", old], &["keybindings", new])?;
        }
        for (old, new) in [
            ("quickResetDelay", "yawResetDelay"),
            ("resetDelay", "fullResetDelay"),
            ("quickResetEnabled", "yawResetEnabled"),
            ("resetEnabled", "fullResetEnabled"),
            ("quickResetTaps", "yawResetTaps"),
            ("resetTaps", "fullResetTaps"),
        ] {
            copy(root, &["tapDetection", old], &["tapDetection", new])?;
        }
    }
    if version < 9 && !get(root, &["skeleton", "offsets", "chestLength"]).is_null() {
        let chest: f32 = read(root, &["skeleton", "offsets", "chestLength"], 0.16)?;
        put(root, &["skeleton", "offsets", "chestLength"], chest / 2.0)?;
        put(
            root,
            &["skeleton", "offsets", "upperChestLength"],
            chest / 2.0,
        )?;
    }
    if version < 10 && equals(root, &["autoBone", "sampleCount"], 1000.0) {
        put(root, &["autoBone", "sampleCount"], 1500)?;
    }
    if version < 11 && root["trackers"]["HMD"].is_mapping() {
        put(root, &["trackers", "HMD", "designation"], "body:head")?;
    }
    if version < 12 {
        if equals(root, &["autoBone", "offsetSlideErrorFactor"], 2.0) {
            put(root, &["autoBone", "offsetSlideErrorFactor"], 1.0)?;
        }
        if equals(root, &["autoBone", "bodyProportionErrorFactor"], 0.825) {
            put(root, &["autoBone", "bodyProportionErrorFactor"], 0.25)?;
        }
    }
    if version < 13 {
        let mut known: Vec<String> = read(root, &["knownDevices"], Vec::new())?;
        if let Some(map) = root["trackers"].as_mapping() {
            for name in map.keys().filter_map(Value::as_str) {
                if let Some((mac, 0)) = tracker_id(name) {
                    if !known.contains(&mac) {
                        known.push(mac);
                    }
                }
            }
        }
        put(root, &["knownDevices"], known)?;
    }
    if version < 14 {
        copy(
            root,
            &["autoBone", "targetHmdHeight"],
            &["skeleton", "hmdHeight"],
        )?;
        if equals(root, &["autoBone", "offsetSlideErrorFactor"], 1.0)
            && equals(root, &["autoBone", "slideErrorFactor"], 0.0)
        {
            put(root, &["autoBone", "offsetSlideErrorFactor"], 0.0)?;
            put(root, &["autoBone", "slideErrorFactor"], 1.0)?;
        }
        if equals(root, &["autoBone", "bodyProportionErrorFactor"], 0.25) {
            put(root, &["autoBone", "bodyProportionErrorFactor"], 0.05)?;
        }
        if equals(root, &["autoBone", "numEpochs"], 100.0) {
            put(root, &["autoBone", "numEpochs"], 50)?;
        }
    }
    if version < 15 && get(root, &["trackingChecklist", "ignoredStepsIds"]).is_sequence() {
        put(
            root,
            &["trackingChecklist", "ignoredStepsIds"],
            Vec::<String>::new(),
        )?;
    }
    put(root, &["version"], VERSION.to_string())
}
