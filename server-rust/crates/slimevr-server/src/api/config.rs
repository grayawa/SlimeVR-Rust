//! Frontend settings and validation; YAML persistence stays in crate::config.
use super::diagnostics;
use crate::receiver::normalize_mac;
use serde::{Deserialize, Serialize};
use slimevr_core::{autobone::AutoBoneConfig, pose::PoseConfig};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FrontendConfig {
    pub version: u32,
    pub pose: PoseConfig,
    pub allowed_macs: Vec<String>,
    pub device_ids: BTreeMap<String, u8>,
    pub tracker_names: BTreeMap<String, String>,
    pub auto_bone: AutoBoneConfig,
    pub magnetometers_enabled: bool,
    pub mag_preferences: BTreeMap<String, bool>,
    pub save_recordings: bool,
    pub ignored_steps: BTreeSet<u8>,
    pub sample_count: usize,
    pub sample_ms: u64,
    pub steam_vr: crate::steamvr::Settings,
    pub osc: crate::osc::Settings,
    /// Original YAML tree; unsupported settings survive updates and resets.
    #[serde(skip)]
    pub yaml: serde_yaml_ng::Value,
    #[serde(skip, default = "default_tracker_port")]
    pub tracker_port: u16,
}
fn default_tracker_port() -> u16 {
    6969
}
impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            version: 1,
            pose: Default::default(),
            allowed_macs: Vec::new(),
            device_ids: BTreeMap::new(),
            tracker_names: BTreeMap::new(),
            auto_bone: Default::default(),
            magnetometers_enabled: false,
            mag_preferences: BTreeMap::new(),
            save_recordings: false,
            ignored_steps: BTreeSet::new(),
            sample_count: 1000,
            sample_ms: 20,
            steam_vr: Default::default(),
            osc: Default::default(),
            yaml: serde_yaml_ng::Value::Mapping(Default::default()),
            tracker_port: 6969,
        }
    }
}
impl FrontendConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported frontend state version".into());
        }
        if self
            .ignored_steps
            .iter()
            .any(|id| !diagnostics::ignorable(*id))
        {
            return Err("invalid ignored checklist step".into());
        }
        crate::hotkeys::Settings::read(&self.yaml)?;
        self.osc.validate()?;
        self.pose.validate()?;
        self.auto_bone.validate()?;
        if self.steam_vr.trackers.len() > 64 || self.steam_vr.trackers.keys().any(|k| k.len() > 128)
        {
            return Err("invalid SteamVR sharing settings".into());
        }
        if !(3..=5000).contains(&self.sample_count)
            || !(4..=1000).contains(&self.sample_ms)
            || self.device_ids.len() > 254
            || self.allowed_macs.len() > 1024
            || self.tracker_names.len() > 16384
            || self.mag_preferences.len() > 16384
            || self.mag_preferences.keys().any(|k| k.len() > 128)
        {
            return Err("frontend state limits exceeded".into());
        }
        let mut ids = BTreeSet::new();
        for id in self.device_ids.values() {
            if *id == 0 || *id == 255 || !ids.insert(id) {
                return Err("invalid or duplicate device id".into());
            }
        }
        for mac in &self.allowed_macs {
            normalize_mac(mac)?;
        }
        if self.tracker_names.values().any(|s| s.len() > 128) {
            return Err("tracker name too long".into());
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        crate::config::load(path).map_err(Into::into)
    }
    pub fn save(&self, path: Option<&Path>) -> Result<(), String> {
        crate::config::save(self, path).map_err(|e| e.to_string())
    }
}
