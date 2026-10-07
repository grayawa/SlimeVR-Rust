use slimevr_gpui::{
    battery::{Color, Marker, Reading},
    protocol::{self, Tracker, Update},
};
use solarxr_protocol::{self as sx, data_feed as df, datatypes as dt, flatbuffers as fb};

fn tracker(percent: u8, voltage: Option<f32>, runtime_us: Option<u64>) -> Tracker {
    Tracker {
        battery: Some(percent),
        voltage,
        battery_runtime_us: runtime_us,
        status: 2,
        ..Default::default()
    }
}
fn reading(percent: u8) -> Reading {
    Reading::from_tracker(&tracker(percent, None, None)).unwrap()
}
#[test]
fn percentages_preserve_empty_and_original_full_charge_sentinels() {
    for (raw, normalized) in [
        (0, 0),
        (7, 7),
        (82, 82),
        (100, 100),
        (101, 100),
        (200, 100),
        (201, 0),
        (255, 0),
    ] {
        let r = reading(raw);
        assert_eq!(r.percent, normalized);
        assert_eq!(r.lines(false), [format!("{normalized}%")]);
        assert!((r.fill() - f32::from(normalized) / 100.).abs() < 0.0001);
    }
}
#[test]
fn colors_follow_original_strict_twenty_and_forty_percent_boundaries() {
    for (raw, color) in [
        (0, Color::Background),
        (1, Color::Critical),
        (20, Color::Critical),
        (21, Color::Warning),
        (40, Color::Warning),
        (41, Color::Success),
        (100, Color::Success),
    ] {
        assert_eq!(reading(raw).color(), color);
    }
    let mut r = reading(82);
    r.disabled = true;
    assert_eq!(r.color(), Color::Disabled);
}
#[test]
fn charging_voltage_and_full_charge_markers_match_original() {
    assert!(
        !Reading::from_tracker(&tracker(80, Some(4.3), None))
            .unwrap()
            .charging()
    );
    for (raw, marker) in [
        (0, Marker::Charging),
        (100, Marker::Charging),
        (101, Marker::Charged),
        (200, Marker::Charged),
        (255, Marker::Charging),
    ] {
        let r = Reading::from_tracker(&tracker(raw, Some(4.5), None)).unwrap();
        assert_eq!(r.marker(), marker);
        assert_eq!(r.fill(), 1.);
        assert_eq!(r.color(), Color::Success);
        assert!(r.lines(false).is_empty());
    }
}
#[test]
fn runtime_is_in_microseconds_and_debug_adds_the_percentage() {
    let r = Reading::from_tracker(&tracker(82, Some(3.9), Some(5_430_000_000))).unwrap();
    assert_eq!(r.runtime_text().as_deref(), Some("1h 30min"));
    assert_eq!(r.lines(false), ["1h 30min"]);
    assert_eq!(r.lines(true), ["1h 30min", "82%"]);
    assert_eq!(
        Reading::from_tracker(&tracker(82, None, Some(0)))
            .unwrap()
            .lines(false),
        ["82%"]
    );
}
#[test]
fn missing_battery_is_hidden_and_invalid_voltage_cannot_indicate_charging() {
    assert!(Reading::from_tracker(&Tracker::default()).is_none());
    for voltage in [f32::NAN, f32::INFINITY, -1., 0.] {
        let r = Reading::from_tracker(&tracker(82, Some(voltage), None)).unwrap();
        assert!(!r.charging());
        assert!(r.voltage.is_none());
    }
    for locale in ["zh-Hans", "en", "de"] {
        let l10n = slimevr_gpui::i18n::Localizer::new(locale).unwrap();
        for key in [
            "native-battery-charging",
            "native-battery-charged",
            "native-battery-voltage",
            "native-battery-runtime",
        ] {
            assert_ne!(l10n.text(key), key);
        }
    }
}
#[test]
fn solarxr_battery_runtime_and_optional_readings_are_decoded_without_unit_conversion() {
    for (percent, voltage, runtime, expected_runtime) in [
        (
            Some(82),
            Some(3.9),
            Some(5_430_000_000),
            Some(5_430_000_000),
        ),
        (Some(255), Some(4.5), Some(-1), None),
        (None, None, None, None),
    ] {
        let mut f = fb::FlatBufferBuilder::new();
        let device_id = dt::DeviceId::new(1);
        let id = dt::TrackerId::create(
            &mut f,
            &dt::TrackerIdArgs {
                device_id: Some(&device_id),
                tracker_num: 0,
            },
        );
        let tracker = df::tracker::TrackerData::create(
            &mut f,
            &df::tracker::TrackerDataArgs {
                tracker_id: Some(id),
                status: dt::TrackerStatus::OK,
                ..Default::default()
            },
        );
        let trackers = f.create_vector(&[tracker]);
        let status = dt::hardware_info::HardwareStatus::create(
            &mut f,
            &dt::hardware_info::HardwareStatusArgs {
                battery_pct_estimate: percent,
                battery_voltage: voltage,
                battery_runtime_estimate: runtime,
                ..Default::default()
            },
        );
        let device = df::device_data::DeviceData::create(
            &mut f,
            &df::device_data::DeviceDataArgs {
                id: Some(&device_id),
                hardware_status: Some(status),
                trackers: Some(trackers),
                ..Default::default()
            },
        );
        let devices = f.create_vector(&[device]);
        let update = df::DataFeedUpdate::create(
            &mut f,
            &df::DataFeedUpdateArgs {
                devices: Some(devices),
                ..Default::default()
            },
        );
        let header = df::DataFeedMessageHeader::create(
            &mut f,
            &df::DataFeedMessageHeaderArgs {
                message_type: df::DataFeedMessage::DataFeedUpdate,
                message: Some(update.as_union_value()),
            },
        );
        let headers = f.create_vector(&[header]);
        let bundle = sx::MessageBundle::create(
            &mut f,
            &sx::MessageBundleArgs {
                data_feed_msgs: Some(headers),
                ..Default::default()
            },
        );
        f.finish(bundle, None);
        let decoded = protocol::decode(f.finished_data()).unwrap();
        let feed = decoded
            .iter()
            .find_map(|u| {
                if let Update::Feed(feed) = u {
                    Some(feed)
                } else {
                    None
                }
            })
            .unwrap();
        let tracker = &feed.trackers[0];
        assert_eq!(tracker.battery, percent);
        assert_eq!(tracker.voltage, voltage);
        assert_eq!(tracker.battery_runtime_us, expected_runtime);
    }
}
