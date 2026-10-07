//! UI structure extracted from the existing React components, separate from
//! protocol metadata. Source order, explanatory copy, widgets and limits survive.
use crate::navigation::Section;
use serde_json::Value;
use std::{collections::BTreeSet, sync::OnceLock};
static PANES: OnceLock<Value> = OnceLock::new();
pub fn panes() -> &'static Value {
    PANES.get_or_init(|| {
        serde_json::from_str(include_str!("settings_layout.json"))
            .expect("Generated settings layout")
    })
}
pub fn pane(section: Section) -> Option<&'static Value> {
    let key = match section {
        Section::SteamVr => "steamvr",
        Section::StayAligned => "stayaligned",
        Section::Mechanics => "mechanics",
        Section::Fk => "fksettings",
        Section::Gestures => "gestureControl",
        Section::OscRouter => "router",
        Section::OscVrchat => "vrchat",
        Section::OscVmc => "vmc",
        Section::Notifications => "notifications",
        Section::Behavior => "behavior",
        Section::Appearance => "appearance",
        Section::Advanced => "advanced",
        _ => return None,
    };
    Some(&panes()[key])
}
pub fn paths(node: &Value) -> BTreeSet<String> {
    fn walk(node: &Value, out: &mut BTreeSet<String>) {
        if let Some(path) = node["path"].as_str() {
            out.insert(path.into());
        }
        for n in node["children"].as_array().into_iter().flatten() {
            walk(n, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(node, &mut out);
    out
}
pub fn radio_value(node: &Value) -> Option<u64> {
    let raw = node["value"].as_str()?;
    if let Ok(value) = raw.parse() {
        return Some(value);
    }
    let raw = raw.split('.').nth(1)?;
    crate::rpc_generated::enum_choices(node["field_type"].as_str()?)
        .iter()
        .find(|(name, _)| *name == raw)
        .map(|(_, v)| *v)
}

pub fn locale_name(code: &str) -> &str {
    panes()["__languages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["key"] == code)
        .and_then(|entry| entry["name"].as_str())
        .unwrap_or(code)
}
