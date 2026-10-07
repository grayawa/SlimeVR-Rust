use slimevr_gpui::{
    i18n::Localizer,
    navigation::{Navigation, Page, Section},
    protocol::{self, Command, ResetKind, Update},
};
use solarxr_protocol::{self as sx, flatbuffers as fb};

#[test]
fn existing_translations_and_native_labels_load_in_both_languages() {
    let zh = Localizer::new("zh-Hans").unwrap();
    assert_eq!(zh.text("navbar-home"), "主界面");
    assert_eq!(zh.text("body_part-LEFT_UPPER_LEG"), "左大腿");
    assert_eq!(zh.text("native-connected"), "已连接");
    let en = Localizer::new("en").unwrap();
    assert_ne!(en.text("reset-full"), "reset-full");
    assert_eq!(en.text("native-connected"), "Connected");
    for localizer in [zh, en] {
        for page in Page::SIDEBAR {
            assert_ne!(localizer.text(page.label()), page.label());
        }
        for (_, sections) in Section::GROUPS {
            for section in *sections {
                assert_ne!(localizer.text(section.label()), section.label());
            }
        }
    }
}

#[test]
fn warnings_return_to_actual_source_and_update_source_on_reentry() {
    let mut nav = Navigation::default();
    nav.go(Page::Settings(Section::OscVmc));
    nav.go(Page::VrchatWarnings);
    nav.go(Page::VrchatWarnings);
    assert!(nav.page.in_settings());
    nav.back_from_warning();
    assert_eq!(nav.page, Page::Settings(Section::OscVmc));
    nav.go(Page::Home);
    nav.go(Page::VrchatWarnings);
    nav.back_from_warning();
    assert_eq!(nav.page, Page::Home);
    let mut direct = Navigation::default();
    direct.page = Page::VrchatWarnings;
    direct.back_from_warning();
    assert_eq!(direct.page, Page::Settings(Section::SteamVr));
}

#[test]
fn subscription_requests_two_bounded_feeds_and_no_settings_writes() {
    let bytes = protocol::subscribe();
    let bundle = fb::root::<sx::MessageBundle<'_>>(&bytes).unwrap();
    assert!(bundle.rpc_msgs().is_none());
    let configs = bundle
        .data_feed_msgs()
        .unwrap()
        .get(0)
        .message_as_start_data_feed()
        .unwrap()
        .data_feeds()
        .unwrap();
    assert_eq!(configs.len(), 2);
    assert_eq!(configs.get(0).minimum_time_since_last(), 100);
    assert!(!configs.get(0).bone_mask());
    assert!(configs.get(1).bone_mask());
    assert_eq!(configs.get(1).minimum_time_since_last(), 25);
    assert!(configs.get(0).server_guards_mask());
    assert!(
        configs
            .get(0)
            .data_mask()
            .unwrap()
            .tracker_data()
            .unwrap()
            .linear_acceleration()
    );
}

#[test]
fn reset_keeps_backend_delay_and_carries_transaction_id() {
    let bytes = protocol::encode(&Command::Reset(ResetKind::Full), 234);
    let bundle = fb::root::<sx::MessageBundle<'_>>(&bytes).unwrap();
    let header = bundle.rpc_msgs().unwrap().get(0);
    assert_eq!(header.tx_id().unwrap().id(), 234);
    assert!(header.message_as_reset_request().unwrap().delay().is_none());
}

#[test]
fn malformed_binary_is_rejected_without_unsafe_access() {
    for bytes in [vec![], vec![4, 0, 0, 0], vec![255; 32]] {
        assert!(protocol::decode(&bytes).is_err());
    }
    let bytes = protocol::heartbeat(123);
    let updates = protocol::decode(&bytes).unwrap();
    assert!(!updates.iter().any(|u| matches!(u, Update::Feed(_))));
}
