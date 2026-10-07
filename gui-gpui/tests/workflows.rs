use serde_json::{Value, json};
use slimevr_gpui::{
    desktop::{Paths, Preferences, SingleInstance},
    json_form, overlay, proportions, rpc_generated as rpc,
    settings::Draft,
    sounds::{Cue, Sequencer},
};
#[test]
fn every_rpc_roundtrips_through_original_bindings() {
    let mut count = 0;
    for name in rpc::RPC_NAMES {
        if *name == "NONE" {
            continue;
        }
        let bytes = rpc::encode_rpc(name, &json!({}), 91).unwrap_or_else(|e| panic!("{name}: {e}"));
        let rows = rpc::decode_rpc(&bytes).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((&rows[0].0, rows[0].1), (&name.to_string(), 91));
        count += 1;
    }
    assert_eq!(count, 86);
}
#[test]
fn invalid_input_cannot_be_truncated_into_protocol_values() {
    for value in [
        json!({"reset_type":255}),
        json!({"delay":1e100}),
        json!({"body_parts":false}),
    ] {
        assert!(rpc::encode_rpc("ResetRequest", &value, 0).is_err());
    }
    assert!(rpc::encode_rpc("AssignTrackerRequest", &json!({"display_name":42}), 0).is_err());
    assert!(rpc::encode_rpc("SettingsRequest", &json!(false), 0).is_err());
}
#[test]
fn settings_drafts_preserve_unsaved_edits_and_confirm_only_matching_session() {
    let mut draft = Draft::default();
    draft.merge(&json!({"filtering":{"type":1,"amount":0.2},"resets_settings":{"save_mounting_reset":true}}));
    draft.set("/filtering/amount", json!(0.8));
    draft.merge(&json!({"filtering":{"type":2,"amount":0.3}}));
    assert_eq!(draft.value["filtering"]["amount"], 0.8);
    assert_eq!(draft.value["filtering"]["type"], 2);
    assert_eq!(
        draft.request().unwrap(),
        json!({"filtering":{"type":2,"amount":0.8}})
    );
    draft.begin_save(1);
    assert!(!draft.confirm(2, &draft.value.clone()));
    assert!(!draft.dirty.is_empty());
    draft.begin_save(2);
    draft.set("/filtering/amount", json!(0.9));
    assert!(draft.confirm(2, &json!({"filtering":{"amount":0.8}})));
    assert!(draft.dirty.contains("/filtering/amount"));
    draft.begin_save(2);
    assert!(draft.confirm(2, &json!({"filtering":{"amount":0.90000001}})));
    assert!(draft.dirty.is_empty());
}
#[test]
fn duplicate_reset_packets_never_replay_second_cue() {
    let mut sound = Sequencer::default();
    assert_eq!(sound.reset(1, 9, 1, false, 0), vec![Cue::Initial(1)]);
    assert_eq!(sound.reset(1, 9, 1, false, 1000), vec![Cue::Tick(1, 1)]);
    for progress in [1000, 1033, 1500, 1999, 0] {
        assert!(sound.reset(1, 9, 1, false, progress).is_empty());
    }
    assert_eq!(sound.reset(1, 9, 1, true, 3000), vec![Cue::Finished(1)]);
    assert!(sound.reset(1, 9, 1, true, 3000).is_empty());
    assert_eq!(sound.reset(1, 10, 1, false, 0), vec![Cue::Initial(1)]);
    assert!(sound.pause(false).is_none());
    assert_eq!(sound.pause(true), Some(Cue::Pause(true)));
    assert!(sound.pause(true).is_none());
}
#[test]
fn existing_preferences_preserve_unknown_keys_and_single_instance_signals_show() {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths {
        root: temp.path().into(),
        logs: temp.path().join("logs"),
        config: temp.path().join("vrconfig.yml"),
        resources: temp.path().into(),
    };
    std::fs::write(paths.root.join("settings.json"),json!({"other":"retained","config.json":json!({"lang":"en","futureKey":[1,2],"fonts":["lexend"]}).to_string()}).to_string()).unwrap();
    let mut prefs = Preferences::load(&paths).unwrap();
    prefs.value["theme"] = json!("light");
    prefs.save().unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&prefs.path).unwrap()).unwrap();
    assert_eq!(saved["other"], "retained");
    let loaded = Preferences::load(&paths).unwrap();
    assert_eq!(loaded.value["futureKey"], json!([1, 2]));
    let first = SingleInstance::acquire(&paths).unwrap().unwrap();
    assert!(SingleInstance::acquire(&paths).unwrap().is_none());
    assert!(first.requested());
    drop(first);
    assert!(SingleInstance::acquire(&paths).unwrap().is_some());
    assert!(!paths.config.exists());
}
#[test]
fn proportions_import_rejects_duplicates_and_ratios_preserve_total() {
    let skeleton = json!({"user_height":1.7,"skeleton_parts":[{"bone":9,"value":0.45},{"bone":10,"value":0.45}]});
    let out = proportions::export(&skeleton);
    assert_eq!(proportions::import(&out).unwrap().len(), 3);
    let mut duplicate = out;
    duplicate["skeletonParts"][1] = duplicate["skeletonParts"][0].clone();
    assert!(proportions::import(&duplicate).is_err());
    let ratio = proportions::ratio(&skeleton, &[9, 10], 9, 0.6).unwrap();
    assert!(
        (ratio[0].1["value"].as_f64().unwrap() + ratio[1].1["value"].as_f64().unwrap() - 0.9).abs()
            < 1e-6
    );
    assert!(proportions::change(9, f64::NAN).is_err());
    assert!(proportions::change(200, 0.5).is_err());
}
#[test]
fn firmware_schema_variants_resolve_refs_and_validate_before_build() {
    let schema = json!({"$defs":{"sensor":{"type":"object","required":["kind","rate"],"properties":{"kind":{"const":"IMU"},"rate":{"type":"integer","minimum":1,"maximum":400,"default":100}}}},"type":"object","required":["sensor"],"properties":{"sensor":{"oneOf":[{"$ref":"#/$defs/sensor"},{"title":"Disabled","type":"object","properties":{"kind":{"const":"NONE"}}}]}}});
    let data = json_form::populate(&schema, &json!({"sensor":{"kind":"IMU","rate":200}}));
    json_form::validate(&schema, &data).unwrap();
    let fields = json_form::fields(&schema, &data);
    assert!(
        fields
            .iter()
            .any(|f| f.kind == "variant" && f.path == "/sensor")
    );
    let rate = fields.iter().find(|f| f.path == "/sensor/rate").unwrap();
    assert!(json_form::parse(rate, "401").is_err());
    assert_eq!(json_form::parse(rate, "250").unwrap(), 250);
    assert!(json_form::validate(&schema, &json!({"sensor":{"kind":"IMU","rate":0}})).is_err());
}
#[test]
fn overlay_handles_only_matching_topics() {
    let mut state = overlay::Overlay::default();
    state.apply(1,&json!({"u":{"type":"TopicMapping","value":{"id":overlay::topic()["value"],"handle":{"id":5}}}}));
    state.apply(1,&json!({"u":{"type":"Message","value":{"topic":{"type":"TopicHandle","value":{"id":5}},"payload":{"type":"KeyValues","value":{"keys":["is_visible","is_mirrored"],"values":["true","false"]}}}}}));
    assert_eq!(
        state.value,
        json!({"is_visible":true,"is_mirrored":false}).into()
    );
    state.apply(2, &json!({}));
    assert!(state.value.is_none());
}
#[test]
fn glb_json_is_extracted_without_reading_binary_meshes() {
    let temp = tempfile::tempdir().unwrap();
    let mut json = json!({"asset":{"version":"2.0"},"nodes":[]})
        .to_string()
        .into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2u32.to_le_bytes());
    bytes.extend((20u32 + json.len() as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(b"JSON");
    bytes.extend(json);
    let path = temp.path().join("avatar.vrm");
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&slimevr_gpui::avatar::read(&path).unwrap()).unwrap()["asset"]
            ["version"],
        "2.0"
    );
    bytes[4] = 1;
    std::fs::write(&path, bytes).unwrap();
    assert!(slimevr_gpui::avatar::read(&path).is_err());
}
#[test]
fn all_original_locales_and_native_settings_labels_are_usable() {
    for locale in slimevr_gpui::locales::LOCALES {
        let l = slimevr_gpui::i18n::Localizer::new(locale).unwrap();
        assert_ne!(l.text("navbar-home"), "navbar-home");
    }
    for locale in ["en", "zh-Hans"] {
        let l = slimevr_gpui::i18n::Localizer::new(locale).unwrap();
        for (_, sections) in slimevr_gpui::navigation::Section::GROUPS {
            for section in *sections {
                for f in slimevr_gpui::settings::fields(*section) {
                    assert_ne!(l.text(&f.label), f.label, "{locale}: {}", f.path);
                }
            }
        }
    }
}
