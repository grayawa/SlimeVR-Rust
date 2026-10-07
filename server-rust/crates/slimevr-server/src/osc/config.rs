use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Endpoint {
    pub enabled: bool,
    pub port_in: u16,
    pub port_out: u16,
    pub address: String,
}
impl Default for Endpoint {
    fn default() -> Self {
        Self {
            enabled: false,
            port_in: 9001,
            port_out: 9000,
            address: "127.0.0.1".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Vrc {
    #[serde(flatten)]
    pub endpoint: Endpoint,
    pub oscquery_enabled: bool,
    pub trackers: BTreeMap<String, bool>,
}
impl Default for Vrc {
    fn default() -> Self {
        Self {
            endpoint: Endpoint::default(),
            oscquery_enabled: true,
            trackers: [
                ("waist".into(), true),
                ("left_foot".into(), true),
                ("right_foot".into(), true),
            ]
            .into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Vmc {
    #[serde(flatten)]
    pub endpoint: Endpoint,
    pub anchor_hip: bool,
    pub mirror_tracking: bool,
    pub vrm_json: Option<String>,
}
impl Default for Vmc {
    fn default() -> Self {
        Self {
            endpoint: Endpoint {
                port_in: 39540,
                port_out: 39539,
                ..Default::default()
            },
            anchor_hip: true,
            mirror_tracking: false,
            vrm_json: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub router: Endpoint,
    pub vrc: Vrc,
    pub vmc: Vmc,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            router: Endpoint {
                port_in: 9002,
                ..Default::default()
            },
            vrc: Default::default(),
            vmc: Default::default(),
        }
    }
}
impl Settings {
    pub fn from_yaml(root: &serde_yaml_ng::Value) -> Result<Self, String> {
        fn read<T: Serialize + serde::de::DeserializeOwned>(
            root: &serde_yaml_ng::Value,
            name: &str,
            default: T,
        ) -> Result<T, String> {
            let mut value = serde_yaml_ng::to_value(default).map_err(|e| e.to_string())?;
            if let Some(map) = root[name].as_mapping() {
                for (key, val) in map {
                    value
                        .as_mapping_mut()
                        .unwrap()
                        .insert(key.clone(), val.clone());
                }
            } else if !root[name].is_null() {
                return Err(format!("{name} must be a map"));
            }
            serde_yaml_ng::from_value(value).map_err(|e| e.to_string())
        }
        Ok(Self {
            router: read(root, "oscRouter", Self::default().router)?,
            vrc: read(root, "vrcOSC", Vrc::default())?,
            vmc: read(root, "vmc", Vmc::default())?,
        })
    }
    pub fn write_yaml(&self, root: &mut serde_yaml_ng::Value) -> std::io::Result<()> {
        for (name, val) in [
            ("oscRouter", serde_yaml_ng::to_value(&self.router)),
            ("vrcOSC", serde_yaml_ng::to_value(&self.vrc)),
            ("vmc", serde_yaml_ng::to_value(&self.vmc)),
        ] {
            let val = val.map_err(std::io::Error::other)?;
            for (k, v) in val.as_mapping().unwrap() {
                crate::config::put(root, &[name, k.as_str().unwrap()], v)?;
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        for e in [&self.router, &self.vrc.endpoint, &self.vmc.endpoint] {
            if e.address.is_empty()
                || e.address.len() > 253
                || e.address
                    .bytes()
                    .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
                || e.port_in == 0
                || e.port_out == 0
            {
                return Err("invalid OSC endpoint".into());
            }
        }
        if self.vrc.trackers.len() > 64 {
            return Err("too many OSC roles".into());
        }
        if let Some(json) = &self.vmc.vrm_json {
            if json.len() > 4 * 1024 * 1024 {
                return Err("VRM JSON exceeds 4 MiB".into());
            }
            super::armature::vrm_offsets(json)?;
        }
        Ok(())
    }
}
