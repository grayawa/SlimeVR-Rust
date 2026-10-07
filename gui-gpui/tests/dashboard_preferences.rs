use serde_json::json;
use slimevr_gpui::{
    dashboard,
    desktop::{Paths, Preferences},
    protocol::{Feed, Tracker},
    tracker_list::Settings,
};

#[test]
fn list_policy_matches_desktop_groups_sorting_and_debug_filter() {
    let feed = Feed {
        trackers: vec![
            Tracker {
                name: "Z".into(),
                body: 3,
                ..Default::default()
            },
            Tracker {
                name: "unassigned".into(),
                ..Default::default()
            },
            Tracker {
                name: "A".into(),
                body: 3,
                ..Default::default()
            },
            Tracker {
                name: "computed".into(),
                body: 3,
                computed: true,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut preferences = Preferences::defaults();
    let groups = Settings::from_preferences(&preferences).groups(&feed);
    assert_eq!(
        groups[0]
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["Z", "A"]
    );
    assert_eq!(groups[1][0].name, "unassigned");
    preferences["homeLayout"] = json!("table");
    preferences["debug"] = json!(true);
    preferences["devSettings"]["sortByName"] = json!(true);
    preferences["devSettings"]["moreInfo"] = json!(true);
    let list = Settings::from_preferences(&preferences);
    assert!(list.table && list.more_info);
    assert_eq!(
        list.groups(&feed)[0]
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "Z", "computed"]
    );
    preferences["devSettings"]["filterSlimesAndHMD"] = json!(true);
    assert_eq!(
        Settings::from_preferences(&preferences).groups(&feed)[0].len(),
        2
    );
}

#[test]
fn resizing_preserves_latest_gui_preferences_and_unknown_store_keys() {
    let directory = tempfile::tempdir().unwrap();
    let paths = Paths {
        root: directory.path().into(),
        logs: directory.path().join("logs"),
        config: directory.path().join("vrconfig.yml"),
        resources: directory.path().into(),
    };
    // Desktop settings can change after the overlay starts.
    std::fs::write(
        paths.root.join("settings.json"),
        serde_json::to_vec(&json!({
        "other-store-key": {"keep":true},
        "steamvrDashboard":{"futureSetting":7},
        "config.json": json!({"homeLayout":"table", "lang":"ja", "theme":"custom",
            "devSettings":{"sortByName":true}}).to_string()
        }))
        .unwrap(),
    )
    .unwrap();
    let mut old_desktop = Preferences::load(&paths).unwrap();
    dashboard::save_width(&paths, 2.2).unwrap();
    old_desktop.value["lang"] = json!("fr");
    old_desktop.save().unwrap();
    let preferences = Preferences::load(&paths).unwrap();
    assert_eq!(preferences.value["homeLayout"], "table");
    assert_eq!(preferences.value["lang"], "fr");
    assert_eq!(preferences.value["theme"], "custom");
    assert_eq!(
        preferences.store_value(dashboard::STORE_KEY)["futureSetting"],
        7
    );
    assert_eq!(
        dashboard::saved_width(preferences.store_value(dashboard::STORE_KEY)),
        2.2
    );
    let store: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&preferences.path).unwrap()).unwrap();
    assert_eq!(store["other-store-key"]["keep"], true);
    let before = std::fs::read(&preferences.path).unwrap();
    for width in [f32::NAN, f32::INFINITY, 0.49, 3.01] {
        assert!(dashboard::save_width(&paths, width).is_err());
    }
    assert_eq!(std::fs::read(&preferences.path).unwrap(), before);
}

#[test]
fn corrupt_or_missing_saved_width_uses_larger_default() {
    for value in [
        json!({}),
        json!({"widthMeters":"bad"}),
        json!({"widthMeters":99}),
    ] {
        assert_eq!(dashboard::saved_width(&value), 1.8);
    }
    assert!(dashboard::valid_width(0.5) && dashboard::valid_width(3.0));
}
