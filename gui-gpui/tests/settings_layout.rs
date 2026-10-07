use serde_json::{Value, json};
use slimevr_gpui::{
    desktop::{Paths, Preferences},
    navigation::Section,
    settings::Draft,
    settings_layout,
};
fn controls(n: &Value, out: &mut Vec<Value>) {
    if n["kind"] == "control" {
        out.push(n.clone());
    }
    for child in n["children"].as_array().into_iter().flatten() {
        controls(child, out);
    }
}
#[test]
fn steamvr_auto_is_separate_from_ordered_body_controls_and_saves_original_field() {
    let pane = settings_layout::pane(Section::SteamVr).unwrap();
    let mut c = Vec::new();
    controls(pane, &mut c);
    assert_eq!(c[0]["path"], "/steam_vr_trackers/automaticTrackerToggle");
    assert_eq!(c[1]["path"], "/steam_vr_trackers/chest");
    assert_eq!(c[2]["path"], "/steam_vr_trackers/waist");
    assert_eq!(c.len(), 11);
    let mut draft = Draft::default();
    draft.merge(&json!({"steam_vr_trackers":{"automaticTrackerToggle":true,"chest":false}}));
    draft.set("/steam_vr_trackers/automaticTrackerToggle", json!(false));
    draft.set("/steam_vr_trackers/chest", json!(true));
    let request = draft.request().unwrap();
    assert_eq!(
        request["steam_vr_trackers"]["automaticTrackerToggle"],
        false
    );
    assert_eq!(request["steam_vr_trackers"]["chest"], true);
    assert!(
        pane.to_string()
            .contains("settings-general-steamvr-description")
    );
    assert!(
        pane.to_string()
            .contains("settings-general-steamvr-trackers-tracker_toggling-description")
    );
}
#[test]
fn original_radios_have_protocol_values_and_fk_owns_mounting_reset_controls() {
    for section in [Section::Mechanics, Section::Fk] {
        let mut c = Vec::new();
        controls(settings_layout::pane(section).unwrap(), &mut c);
        for radio in c.iter().filter(|n| n["widget"] == "Radio") {
            assert!(settings_layout::radio_value(radio).is_some(), "{radio}");
        }
    }
    let fk = settings_layout::paths(settings_layout::pane(Section::Fk).unwrap());
    assert!(fk.contains("/resets_settings/reset_mounting_feet"));
    assert!(fk.contains("/resets_settings/reset_hmd_pitch"));
    assert!(
        !settings_layout::paths(settings_layout::pane(Section::Mechanics).unwrap())
            .contains("/resets_settings/reset_mounting_feet")
    );
}
#[test]
fn resetting_gui_preferences_preserves_language_identity_and_unknown_keys() {
    let root = std::env::temp_dir().join(format!("slimevr-reset-prefs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let paths = Paths {
        root: root.clone(),
        logs: root.join("logs"),
        config: root.join("vrconfig.yml"),
        resources: root.join("resources"),
    };
    let mut prefs = Preferences::load(&paths).unwrap();
    prefs.value["lang"] = json!("ja");
    prefs.value["uuid"] = json!("existing-identity");
    prefs.value["custom"] = json!({"keep":1});
    prefs.value["devSettings"]["future_key"] = json!(7);
    prefs.value["theme"] = json!("light");
    prefs.reset_known().unwrap();
    let loaded = Preferences::load(&paths).unwrap();
    assert_eq!(loaded.value["lang"], "ja");
    assert_eq!(loaded.value["uuid"], "existing-identity");
    assert_eq!(loaded.value["custom"]["keep"], 1);
    assert_eq!(loaded.value["devSettings"]["future_key"], 7);
    assert_eq!(loaded.value["theme"], "slime");
    std::fs::remove_dir_all(root).unwrap();
}
