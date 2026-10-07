use serde_json::json;
use slimevr_gpui::onboarding::{Setup, Step, TrackerSet};

#[test]
fn tracker_sets_use_the_original_connection_branches() {
    for set in [TrackerSet::Regular, TrackerSet::Wifi] {
        assert_eq!(set.connection(), Step::Wifi);
    }
    for set in [TrackerSet::Butterfly, TrackerSet::Dongle] {
        assert_eq!(set.connection(), Step::Dongle);
    }
}

#[test]
fn finish_preserves_unrelated_settings_and_applies_current_choices() {
    let current = json!({"model_settings":{"toggles":{"extended_spine":true,"self_localization":true}},"resets_settings":{"yaw_reset_smooth_time":0.4},"vrc_osc":{"osc_settings":{"port_out":9000,"address":"127.0.0.1","enabled":true}}});
    let mut setup = Setup::default();
    let v = setup.settings(&current);
    assert_eq!(v["model_settings"]["toggles"]["extended_spine"], true);
    assert_eq!(v["model_settings"]["toggles"]["self_localization"], false);
    assert_eq!(v["vrc_osc"]["osc_settings"]["port_out"], 9000);
    assert_eq!(v["vrc_osc"]["osc_settings"]["enabled"], false);
    setup.standalone = true;
    assert_eq!(
        setup.settings(&current)["vrc_osc"]["osc_settings"]["enabled"],
        true
    );
    setup.mocap = true;
    setup.standing = Some(true);
    setup.forehead = Some(true);
    let v = setup.settings(&current);
    assert_eq!(v["model_settings"]["toggles"]["self_localization"], true);
    assert_eq!(v["resets_settings"]["reset_hmd_pitch"], true);
    assert_eq!(v["resets_settings"]["yaw_reset_smooth_time"], 0.4);
    assert_eq!(v["vrc_osc"]["osc_settings"]["enabled"], false);
}

#[test]
fn mocap_without_head_tracker_can_complete_and_clears_head_settings() {
    let mut setup = Setup {
        mocap: true,
        ..Default::default()
    };
    assert!(!setup.can_finish_mocap());
    setup.standing = Some(false);
    setup.forehead = Some(false);
    assert!(setup.can_finish_mocap());
    setup.head_tracker = false;
    setup.forehead = None;
    setup.standing = None;
    assert!(setup.can_finish_mocap());
    assert_eq!(
        setup.settings(&json!({}))["resets_settings"]["reset_hmd_pitch"],
        false
    );
}

#[test]
fn wifi_accepts_current_response_and_broadcast_but_rejects_old_session_or_sequence() {
    let mut setup = Setup {
        wifi_request: Some((4, 42, 100)),
        ..Default::default()
    };
    assert!(setup.accepts_wifi(4, 42, 101));
    assert!(setup.accepts_wifi(4, 0, 102));
    assert!(!setup.accepts_wifi(3, 42, 101));
    assert!(!setup.accepts_wifi(4, 42, 100));
    assert!(!setup.accepts_wifi(4, 40, 103));
    setup.wifi_request = None;
    assert!(!setup.accepts_wifi(4, 0, 104));
}
