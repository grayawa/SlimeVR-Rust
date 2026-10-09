//! Settings drafts retain edits across navigation and incoming state updates.
//! Saving is an explicit operation in the current connection session.
use crate::{navigation::Section, rpc_generated};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Field {
    pub path: String,
    pub kind: String,
    pub label: String,
}
#[derive(Default)]
pub struct Draft {
    pub value: Value,
    pub dirty: BTreeSet<String>,
    pub invalid: BTreeMap<String, String>,
    pub save_started: Option<std::time::Instant>,
    pub saving: Option<(u64, BTreeMap<String, Value>)>,
}
impl Draft {
    pub fn merge(&mut self, incoming: &Value) {
        fn walk(target: &mut Value, incoming: &Value, path: &str, dirty: &BTreeSet<String>) {
            if let Some(map) = incoming.as_object() {
                if !target.is_object() {
                    *target = json!({});
                }
                for (key, value) in map {
                    let path = format!("{path}/{key}");
                    walk(&mut target[key], value, &path, dirty);
                }
            } else if !dirty.contains(path) {
                *target = incoming.clone();
            }
        }
        walk(&mut self.value, incoming, "", &self.dirty);
    }
    pub fn set(&mut self, path: &str, value: Value) {
        if let Some(field) = self.value.pointer_mut(path) {
            *field = value;
            self.dirty.insert(path.into());
            self.invalid.remove(path);
        }
    }
    pub fn edit(&mut self, field: &Field, text: &str) -> Result<(), String> {
        let parsed = parse(field, text);
        match parsed {
            Ok(value) => {
                self.set(&field.path, value);
                Ok(())
            }
            Err(error) => {
                self.invalid.insert(field.path.clone(), error.clone());
                Err(error)
            }
        }
    }
    pub fn request(&self) -> Result<Value, String> {
        if !self.invalid.is_empty() {
            return Err("native-invalid-fields".into());
        }
        if self.dirty.is_empty() {
            return Err("native-no-changes".into());
        }
        let mut out = json!({});
        for path in &self.dirty {
            if let Some(group) = path.split('/').nth(1) {
                out[group] = self.value[group].clone();
            }
        }
        rpc_generated::encode_rpc("ChangeSettingsRequest", &out, 0)?;
        Ok(out)
    }
    pub fn begin_save(&mut self, session: u64) {
        self.save_started = Some(std::time::Instant::now());
        self.saving = Some((
            session,
            self.dirty
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        self.value.pointer(p).cloned().unwrap_or(Value::Null),
                    )
                })
                .collect(),
        ));
    }
    pub fn save_expired(&mut self) -> bool {
        if self.saving.is_some()
            && self
                .save_started
                .is_some_and(|at| at.elapsed() > std::time::Duration::from_secs(8))
        {
            self.saving = None;
            self.save_started = None;
            true
        } else {
            false
        }
    }
    pub fn confirm(&mut self, session: u64, incoming: &Value) -> bool {
        let Some((saved_session, expected)) = self.saving.as_ref() else {
            return false;
        };
        if *saved_session != session {
            self.saving = None;
            return false;
        }
        if !expected
            .iter()
            .all(|(p, v)| incoming.pointer(p).is_some_and(|got| equal(got, v)))
        {
            return false;
        }
        let expected = self.saving.take().unwrap().1;
        for (path, value) in expected {
            if self.value.pointer(&path).is_some_and(|v| equal(v, &value)) {
                self.dirty.remove(&path);
            }
        }
        true
    }
}
fn equal(a: &Value, b: &Value) -> bool {
    a == b
        || a.as_f64()
            .zip(b.as_f64())
            .is_some_and(|(a, b)| (a - b).abs() <= 1e-5)
}
fn parse(field: &Field, text: &str) -> Result<Value, String> {
    let name = field.path.rsplit('/').next().unwrap_or("");
    if field.kind == "string" {
        if name == "address" && text.parse::<std::net::IpAddr>().is_err() {
            return Err("native-invalid-address".into());
        }
        return Ok(json!(text));
    }
    if !rpc_generated::enum_choices(&field.kind).is_empty() {
        let n = text.parse::<u64>().map_err(|_| "native-invalid-number")?;
        if !rpc_generated::enum_choices(&field.kind)
            .iter()
            .any(|(_, v)| *v == n)
        {
            return Err("native-invalid-selection".into());
        }
        return Ok(json!(n));
    }
    let n = text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && n.abs() <= f32::MAX as f64)
        .ok_or("native-invalid-number")?;
    let ratio =
        name == "amount" || name == "correction_strength" || field.path.contains("/ratios/");
    if ratio && !(0.0..=1.0).contains(&n) {
        return Err("native-invalid-ratio".into());
    }
    if (name.contains("delay") || name.contains("smooth_time")) && n < 0.0 {
        return Err("native-invalid-number".into());
    }
    if name == "duration" && n <= 0.0 {
        return Err("native-invalid-number".into());
    }
    if matches!(name, "port_in" | "port_out") && !(1.0..=65535.0).contains(&n) {
        return Err("native-invalid-port".into());
    }
    let kind = field.kind.as_str();
    if kind.starts_with("uint") || matches!(kind, "ubyte" | "ushort" | "ulong") {
        let max = match kind {
            "uint8" | "ubyte" => u8::MAX as f64,
            "uint16" | "ushort" => u16::MAX as f64,
            "uint32" | "uint" => u32::MAX as f64,
            _ => u64::MAX as f64,
        };
        if n < 0.0 || n > max || n.fract() != 0.0 {
            return Err("native-invalid-number".into());
        }
        return Ok(json!(
            text.trim()
                .parse::<u64>()
                .map_err(|_| "native-invalid-number")?
        ));
    }
    if kind.starts_with("int") || matches!(kind, "byte" | "short" | "long") {
        let (min, max) = match kind {
            "int8" | "byte" => (i8::MIN as f64, i8::MAX as f64),
            "int16" | "short" => (i16::MIN as f64, i16::MAX as f64),
            "int32" | "int" => (i32::MIN as f64, i32::MAX as f64),
            _ => (i64::MIN as f64, i64::MAX as f64),
        };
        if n < min || n > max || n.fract() != 0.0 {
            return Err("native-invalid-number".into());
        }
        return Ok(json!(
            text.trim()
                .parse::<i64>()
                .map_err(|_| "native-invalid-number")?
        ));
    }
    Ok(json!(n))
}
pub fn groups(section: Section) -> &'static [&'static str] {
    match section {
        Section::SteamVr => &["steam_vr_trackers"],
        Section::StayAligned => &["stay_aligned"],
        Section::Mechanics => &[
            "filtering",
            "drift_compensation",
            "resets_settings",
            "hid_settings",
        ],
        Section::Fk => &["model_settings", "velocity_settings", "resets_settings"],
        Section::Gestures => &["tap_detection_settings"],
        Section::OscRouter => &["osc_router"],
        Section::OscVrchat => &["vrc_osc"],
        Section::OscVmc => &["vmc_osc"],
        Section::Advanced => &["auto_bone_settings"],
        _ => &[],
    }
}
fn normalized(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
pub fn fields(section: Section) -> Vec<Field> {
    let schema = rpc_generated::schema();
    let labels: Vec<Value> = serde_json::from_str(include_str!("settings_labels.json")).unwrap();
    let overrides: BTreeMap<String, String> =
        serde_json::from_str(include_str!("settings_overrides.json")).unwrap();
    fn walk(
        schema: &Value,
        labels: &[Value],
        overrides: &BTreeMap<String, String>,
        typ: &str,
        path: &str,
        out: &mut Vec<Field>,
    ) {
        if let Some(fields) = schema[typ].as_array() {
            for f in fields {
                let name = f["name"].as_str().unwrap();
                let kind = f["type"].as_str().unwrap();
                let path = format!("{path}/{name}");
                if name == "setup_mode" || name == "setupComplete" || name == "vrm_json" {
                    continue;
                }
                if schema.get(kind).is_some() {
                    walk(schema, labels, overrides, kind, &path, out);
                } else {
                    let norm = normalized(name);
                    let group = path.split('/').nth(1).unwrap_or("");
                    let prefix = match group {
                        "steam_vr_trackers" => "trackers",
                        "tap_detection_settings" => "tapDetection",
                        "osc_router" => "router",
                        "vrc_osc" => "osc",
                        "vmc_osc" => "vmc",
                        _ => group,
                    };
                    let matches: Vec<_> = labels
                        .iter()
                        .filter(|l| {
                            normalized(l["path"].as_str().unwrap().rsplit('.').next().unwrap())
                                == norm
                        })
                        .collect();
                    let label = overrides
                        .get(&path)
                        .cloned()
                        .or_else(|| {
                            matches
                                .iter()
                                .find(|l| {
                                    normalized(
                                        l["path"].as_str().unwrap().split('.').next().unwrap(),
                                    ) == normalized(prefix)
                                })
                                .or_else(|| matches.first())
                                .map(|l| l["label"].as_str().unwrap().to_owned())
                        })
                        .unwrap_or_else(|| {
                            format!(
                                "native-field-{}",
                                path.trim_start_matches('/').replace('/', "-")
                            )
                        });
                    out.push(Field {
                        path,
                        kind: kind.into(),
                        label,
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    for field in schema["SettingsResponse"].as_array().unwrap() {
        let name = field["name"].as_str().unwrap();
        if groups(section).contains(&name) {
            walk(
                &schema,
                &labels,
                &overrides,
                field["type"].as_str().unwrap(),
                &format!("/{name}"),
                &mut out,
            );
        }
    }
    out
}
