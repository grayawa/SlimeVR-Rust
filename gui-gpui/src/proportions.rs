//! Shared validation for native import, scale, ratio and manual body measurement.
use crate::rpc_generated;
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn valid(bone: u64, value: f64) -> bool {
    if !value.is_finite() {
        return false;
    }
    match bone {
        1 | 4 | 7 | 12 | 13 | 18 | 19 | 20 => (-1.0..=1.0).contains(&value),
        2 | 3 | 5 | 6 | 8 | 9 | 10 | 11 | 14 | 15 | 16 | 17 | 21 => (0.001..=3.0).contains(&value),
        _ => false,
    }
}
pub fn change(bone: u64, value: f64) -> Result<(String, Value), String> {
    if !valid(bone, value) {
        return Err("Invalid body measurement".into());
    }
    Ok((
        "ChangeSkeletonConfigRequest".into(),
        json!({"bone":bone,"value":value}),
    ))
}
pub fn export(skeleton: &Value) -> Value {
    let parts = skeleton["skeleton_parts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let name = rpc_generated::enum_choices("SkeletonBone")
                .iter()
                .find(|(_, id)| Some(*id) == p["bone"].as_u64())
                .map(|(n, _)| *n)
                .unwrap_or("NONE");
            json!({"bone":name,"value":p["value"]})
        })
        .collect::<Vec<_>>();
    json!({"version":1,"skeletonParts":parts})
}
pub fn import(value: &Value) -> Result<Vec<(String, Value)>, String> {
    if value.get("version").is_some_and(|v| v.as_u64() != Some(1)) {
        return Err("Unsupported body proportion format".into());
    }
    let parts = value["skeletonParts"]
        .as_array()
        .filter(|p| !p.is_empty() && p.len() <= 21)
        .ok_or("Missing body measurements")?;
    let mut seen = BTreeSet::new();
    let mut requests = Vec::new();
    for part in parts {
        let bone = part["bone"]
            .as_u64()
            .or_else(|| {
                part["bone"].as_str().and_then(|s| {
                    rpc_generated::enum_choices("SkeletonBone")
                        .iter()
                        .find(|(n, _)| *n == s)
                        .map(|(_, v)| *v)
                })
            })
            .ok_or("Unknown bone")?;
        if !seen.insert(bone) {
            return Err("Duplicate body measurement".into());
        }
        requests.push(change(
            bone,
            part["value"].as_f64().ok_or("Missing body measurement")?,
        )?);
    }
    requests.push(("SkeletonConfigRequest".into(), json!({})));
    Ok(requests)
}
pub fn scale(_skeleton: &Value, height: f64) -> Result<Vec<(String, Value)>, String> {
    if !height.is_finite() || !(1.2..=1.936).contains(&height) {
        return Err("Headset height must be between 120 and 193.6 cm".into());
    }
    Ok(vec![
        (
            "ChangeSettingsRequest".into(),
            json!({"model_settings":{"skeleton_height":{"hmd_height":height,"floor_height":0.0}}}),
        ),
        ("SkeletonResetAllRequest".into(), json!({})),
        ("SkeletonConfigRequest".into(), json!({})),
    ])
}
pub const GROUPS: &[(&str, &[u64])] = &[
    ("torso_group", &[21, 3, 6, 5]),
    ("leg_group", &[9, 10]),
    ("arm_group", &[16, 17]),
];
pub fn resize_group(
    skeleton: &Value,
    ids: &[u64],
    length: f64,
) -> Result<Vec<(String, Value)>, String> {
    let parts = skeleton["skeleton_parts"]
        .as_array()
        .ok_or("Body measurements unavailable")?;
    let total: f64 = parts
        .iter()
        .filter(|p| p["bone"].as_u64().is_some_and(|id| ids.contains(&id)))
        .filter_map(|p| p["value"].as_f64())
        .sum();
    if total <= 0.0 || !(0.01..=3.0).contains(&length) {
        return Err("Invalid group length".into());
    }
    let mut requests = Vec::new();
    for p in parts {
        let id = p["bone"].as_u64().unwrap_or(0);
        if ids.contains(&id) {
            requests.push(change(
                id,
                p["value"].as_f64().unwrap_or(0.0) * length / total,
            )?);
        }
    }
    requests.push(("SkeletonConfigRequest".into(), json!({})));
    Ok(requests)
}
pub fn ratio(
    skeleton: &Value,
    ids: &[u64],
    bone: u64,
    ratio: f64,
) -> Result<Vec<(String, Value)>, String> {
    if !ids.contains(&bone) || !(0.01..=0.99).contains(&ratio) {
        return Err("Body ratio must be between 1 and 99%".into());
    }
    let parts = skeleton["skeleton_parts"]
        .as_array()
        .ok_or("Body measurements unavailable")?;
    let total: f64 = parts
        .iter()
        .filter(|p| p["bone"].as_u64().is_some_and(|id| ids.contains(&id)))
        .filter_map(|p| p["value"].as_f64())
        .sum();
    let old = parts
        .iter()
        .find(|p| p["bone"].as_u64() == Some(bone))
        .and_then(|p| p["value"].as_f64())
        .ok_or("Bone unavailable")?;
    if total <= old {
        return Err("Invalid group proportions".into());
    }
    let mut requests = Vec::new();
    for p in parts {
        let id = p["bone"].as_u64().unwrap_or(0);
        if ids.contains(&id) {
            let length = if id == bone {
                total * ratio
            } else {
                p["value"].as_f64().unwrap_or(0.0) * (total * (1.0 - ratio) / (total - old))
            };
            requests.push(change(id, length)?);
        }
    }
    requests.push(("SkeletonConfigRequest".into(), json!({})));
    Ok(requests)
}
