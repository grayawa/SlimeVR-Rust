//! Read VRChat preferences using raw registry bytes; Unity writes doubles as DWORD data.
use super::{protocol::rpc_frame, FrontendConfig};
use slimevr_core::{pose::PoseSnapshot, skeleton::BodyPosition as B};
use solarxr_protocol::rpc;
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Clone, Debug, PartialEq)]
pub struct Values {
    pub legacy: bool,
    pub shoulders: bool,
    pub width: bool,
    pub height: f64,
    pub range: f64,
    pub visuals: bool,
    pub model: u8,
    pub spine: u8,
    pub measurement: u8,
}
pub const FIELDS: [&str; 9] = [
    "legacyModeOk",
    "shoulderTrackingOk",
    "shoulderWidthCompensationOk",
    "userHeightOk",
    "calibrationRangeOk",
    "calibrationVisualsOk",
    "trackerModelOk",
    "spineModeOk",
    "avatarMeasurementTypeOk",
];
impl Values {
    pub fn decode(bytes: &BTreeMap<String, Vec<u8>>) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let find = |key: &str| {
            bytes
                .iter()
                .find(|(name, _)| {
                    name.strip_prefix(key).is_some_and(|suffix| {
                        suffix.is_empty()
                            || suffix.strip_prefix("_h").is_some_and(|s| {
                                !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
                            })
                    })
                })
                .map(|(_, v)| v)
        };
        let int = |key| {
            find(key)
                .filter(|v| v.len() == 4)
                .map(|v| i32::from_le_bytes(v.as_slice().try_into().unwrap()))
        };
        let double = |key| {
            find(key)
                .filter(|v| v.len() == 8)
                .map(|v| f64::from_le_bytes(v.as_slice().try_into().unwrap()))
                .filter(|v| v.is_finite())
                .unwrap_or(-1.0)
        };
        Some(Self {
            legacy: int("VRC_IK_LEGACY") == Some(1),
            shoulders: int("VRC_IK_DISABLE_SHOULDER_TRACKING") == Some(1),
            width: int("VRC_IK_SHOULDER_WIDTH_COMPENSATION") == Some(1),
            height: double("PlayerHeight"),
            range: double("VRC_IK_CALIBRATION_RANGE"),
            visuals: int("VRC_IK_CALIBRATION_VIS") == Some(1),
            model: int("VRC_IK_TRACKER_MODEL")
                .filter(|v| (0..=3).contains(v))
                .map_or(0, |v| v as u8 + 1),
            spine: int("VRC_IK_FBT_SPINE_MODE")
                .filter(|v| (0..=2).contains(v))
                .map_or(0, |v| v as u8 + 1),
            measurement: match int("VRC_IK_AVATAR_MEASUREMENT_TYPE") {
                Some(0) => 2,
                Some(1) => 1,
                _ => 0,
            },
        })
    }
}
pub fn wine_registry(text: &str) -> BTreeMap<String, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut section = false;
    let mut logical = String::new();
    for line in text.lines() {
        let line = line.trim();
        if logical.is_empty() && line.starts_with('[') {
            section = line.starts_with(r"[Software\\VRChat\\VRChat]");
            continue;
        }
        if !section {
            continue;
        }
        logical.push_str(line.trim_end_matches('\\'));
        if line.ends_with('\\') {
            continue;
        }
        let complete = std::mem::take(&mut logical);
        let Some((name, value)) = complete.strip_prefix('"').and_then(|s| s.split_once("\"="))
        else {
            continue;
        };
        let bytes = if let Some(hex) = value.strip_prefix("dword:") {
            u32::from_str_radix(hex, 16)
                .ok()
                .map(|v| v.to_le_bytes().to_vec())
        } else if let Some(hex) = value
            .strip_prefix("hex(4):")
            .or_else(|| value.strip_prefix("hex(b):"))
            .or_else(|| value.strip_prefix("hex:"))
        {
            hex.split(',')
                .map(|v| u8::from_str_radix(v.trim(), 16))
                .collect::<Result<Vec<_>, _>>()
                .ok()
        } else {
            None
        };
        if let Some(bytes) = bytes.filter(|v| v.len() <= 256) {
            result.insert(name.into(), bytes);
        }
    }
    result
}
fn registry_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SLIMEVR_VRC_REGISTRY") {
        return Some(path.into());
    }
    if cfg!(windows) {
        return None;
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let mut roots = vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
    ];
    let mut extras = vec![];
    for root in &roots {
        if let Ok(text) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for line in text.lines() {
                let fields: Vec<_> = line.split('"').collect();
                if fields.get(1) == Some(&"path") {
                    if let Some(path) = fields.get(3) {
                        extras.push(PathBuf::from(path.replace("\\\\", "\\")));
                    }
                }
            }
        }
    }
    roots.extend(extras);
    roots
        .into_iter()
        .map(|r| r.join("steamapps/compatdata/438100/pfx/user.reg"))
        .find(|p| p.is_file())
}
pub fn read() -> Option<Values> {
    if let Some(path) = registry_path() {
        if std::fs::metadata(&path).ok()?.len() > 16 * 1024 * 1024 {
            return None;
        }
        let text = std::fs::read_to_string(path).ok()?;
        return Values::decode(&wine_registry(&text));
    }
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_CURRENT_USER, KEY_READ},
            RegKey,
        };
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(r"Software\VRChat\VRChat", KEY_READ)
            .ok()?;
        let bytes = key
            .enum_values()
            .filter_map(Result::ok)
            .map(|(k, v)| (k, v.bytes))
            .collect();
        Values::decode(&bytes)
    }
    #[cfg(not(windows))]
    None
}
pub fn monitor(commands: tokio::sync::mpsc::Sender<super::Request>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut clock = tokio::time::interval(std::time::Duration::from_secs(3));
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous = None;
        loop {
            clock.tick().await;
            let value = tokio::task::spawn_blocking(read).await.unwrap_or(None);
            if value != previous {
                previous = value.clone();
                if commands.send(super::Request::Vrchat(value)).await.is_err() {
                    break;
                }
            }
        }
    })
}
pub fn validity(v: &Values, c: &FrontendConfig, p: &PoseSnapshot) -> [bool; 9] {
    let assigned = |body| c.pose.bindings.iter().any(|b| b.body == body);
    let positioned = |body| {
        p.trackers.iter().any(|t| {
            t.body == body
                && t.position.is_some()
                && matches!(
                    t.status,
                    slimevr_core::SensorStatus::Ok | slimevr_core::SensorStatus::Busy
                )
        }) || p.positioned_controllers.contains(&body)
    };
    let left = positioned(B::LeftHand);
    let right = positioned(B::RightHand);
    let force = c.pose.skeleton.force_arms_from_hmd;
    let missing_arms = [
        B::LeftLowerArm,
        B::RightLowerArm,
        B::LeftUpperArm,
        B::RightUpperArm,
    ]
    .into_iter()
    .any(|b| !assigned(b));
    let missing_shoulders = !assigned(B::LeftShoulder) || !assigned(B::RightShoulder);
    let shoulders = ((force || !left || !right) || missing_arms)
        && ((!force && left && right) || missing_shoulders);
    let height = slimevr_core::autobone::skeleton_height(c.pose.skeleton) / 0.936;
    [
        !v.legacy,
        v.shoulders == shoulders,
        v.width,
        (height as f64 - v.height).abs() < 0.1,
        (v.range - 0.2).abs() < 0.1,
        v.visuals,
        v.model == 4,
        matches!(v.spine, 1 | 2),
        v.measurement == 1,
    ]
}
pub fn healthy(v: &Values, c: &FrontendConfig, p: &PoseSnapshot) -> bool {
    let muted = c.yaml["vrcConfig"]["mutedWarnings"].as_sequence();
    validity(v, c, p)
        .into_iter()
        .zip(FIELDS)
        .all(|(ok, name)| ok || muted.is_some_and(|m| m.iter().any(|v| v.as_str() == Some(name))))
}
pub fn frame(tx: u32, v: Option<&Values>, c: &FrontendConfig, p: &PoseSnapshot) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::VRCConfigStateChangeResponse, tx, |f| {
        let Some(v) = v else {
            return rpc::VRCConfigStateChangeResponse::create(f, &Default::default())
                .as_union_value();
        };
        let validity = validity(v, c, p);
        let height = slimevr_core::autobone::skeleton_height(c.pose.skeleton) / 0.936;
        let current = rpc::VRCConfigValues::create(
            f,
            &rpc::VRCConfigValuesArgs {
                legacy_mode: v.legacy,
                shoulder_tracking_disabled: v.shoulders,
                shoulder_width_compensation: v.width,
                user_height: v.height as f32,
                calibration_range: v.range as f32,
                calibration_visuals: v.visuals,
                tracker_model: rpc::VRCTrackerModel(v.model),
                spine_mode: rpc::VRCSpineMode(v.spine),
                avatar_measurement_type: rpc::VRCAvatarMeasurementType(v.measurement),
            },
        );
        let valid = rpc::VRCConfigValidity::create(
            f,
            &rpc::VRCConfigValidityArgs {
                legacy_mode_ok: validity[0],
                shoulder_tracking_ok: validity[1],
                shoulder_width_compensation_ok: validity[2],
                user_height_ok: validity[3],
                calibration_range_ok: validity[4],
                calibration_visuals_ok: validity[5],
                tracker_model_ok: validity[6],
                spine_mode_ok: validity[7],
                avatar_measurement_type_ok: validity[8],
            },
        );
        let spine = f.create_vector(&[rpc::VRCSpineMode::LOCK_HIP, rpc::VRCSpineMode::LOCK_HEAD]);
        let recommended = rpc::VRCConfigRecommendedValues::create(
            f,
            &rpc::VRCConfigRecommendedValuesArgs {
                legacy_mode: false,
                shoulder_tracking_disabled: if validity[1] {
                    v.shoulders
                } else {
                    !v.shoulders
                },
                shoulder_width_compensation: true,
                user_height: height,
                calibration_range: 0.2,
                calibration_visuals: true,
                tracker_model: rpc::VRCTrackerModel::AXIS,
                spine_mode: Some(spine),
                avatar_measurement_type: rpc::VRCAvatarMeasurementType::HEIGHT,
            },
        );
        let strings = c.yaml["vrcConfig"]["mutedWarnings"]
            .as_sequence()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .map(|s| f.create_string(s))
            .collect::<Vec<_>>();
        let muted = f.create_vector(&strings);
        rpc::VRCConfigStateChangeResponse::create(
            f,
            &rpc::VRCConfigStateChangeResponseArgs {
                is_supported: true,
                validity: Some(valid),
                state: Some(current),
                recommended: Some(recommended),
                muted: Some(muted),
            },
        )
        .as_union_value()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unity_hashed_keys_and_wine_raw_doubles_decode_without_dword_truncation() {
        let text = r#"WINE REGISTRY Version 2
[Software\\VRChat\\VRChat] 1
"VRC_IK_LEGACY_h42"=dword:00000000
"VRC_IK_SHOULDER_WIDTH_COMPENSATION_h15"=dword:00000001
"PlayerHeight_h123"=hex(4):00,00,00,00,00,00,fc,3f
"VRC_IK_CALIBRATION_RANGE_h64"=hex(b):9a,99,99,99,99,99,c9,3f
"VRC_IK_TRACKER_MODEL_h2"=dword:00000003
"VRC_IK_AVATAR_MEASUREMENT_TYPE_h3"=dword:00000001
[Other] 2
"PlayerHeight"=dword:00000000
"#;
        let v = Values::decode(&wine_registry(text)).unwrap();
        assert_eq!(v.height, 1.75);
        assert!((v.range - 0.2).abs() < 1e-10);
        assert!(v.width);
        assert_eq!(v.model, 4);
        assert_eq!(v.measurement, 1);
        assert!(!v.legacy);
        assert!(Values::decode(&BTreeMap::new()).is_none());
    }
}
