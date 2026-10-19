use slimevr_gpui::{
    i18n::Localizer,
    protocol::{self, Update},
    stay_aligned::{Pose, Step},
};
use solarxr_protocol::{self as sx, data_feed as df, datatypes as dt, flatbuffers as fb};

#[test]
fn tracker_debug_information_survives_the_solarxr_feed() {
    let mut f = fb::FlatBufferBuilder::new();
    let device_id = dt::DeviceId::new(1);
    let id = dt::TrackerId::create(
        &mut f,
        &dt::TrackerIdArgs {
            device_id: Some(&device_id),
            tracker_num: 0,
        },
    );
    let aligned = df::stay_aligned::StayAlignedTracker::create(
        &mut f,
        &df::stay_aligned::StayAlignedTrackerArgs {
            yaw_correction_in_deg: 12.5,
            locked_error_in_deg: 1.5,
            center_error_in_deg: 2.5,
            neighbor_error_in_deg: 3.5,
            locked: true,
        },
    );
    let tracker = df::tracker::TrackerData::create(
        &mut f,
        &df::tracker::TrackerDataArgs {
            tracker_id: Some(id),
            status: dt::TrackerStatus::OK,
            stay_aligned: Some(aligned),
            ..Default::default()
        },
    );
    let trackers = f.create_vector(&[tracker]);
    let device = df::device_data::DeviceData::create(
        &mut f,
        &df::device_data::DeviceDataArgs {
            id: Some(&device_id),
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
    let updates = protocol::decode(f.finished_data()).unwrap();
    let feed = updates
        .iter()
        .find_map(|u| {
            if let Update::Feed(f) = u {
                Some(f)
            } else {
                None
            }
        })
        .unwrap();
    let tracker = &feed.trackers[0];
    assert_eq!(tracker.yaw_correction, Some(12.5));
    let info = tracker.stay_aligned.as_ref().unwrap();
    assert_eq!(
        (
            info.correction,
            info.locked_error,
            info.center_error,
            info.neighbor_error
        ),
        (12.5, 1.5, 2.5, 3.5)
    );
    assert!(info.locked);
}

#[test]
fn seven_steps_and_pose_dialogs_reuse_original_localized_instructions() {
    for locale in ["en", "zh-Hans", "de"] {
        let l10n = Localizer::new(locale).unwrap();
        for step in Step::ALL {
            assert_ne!(l10n.text(step.title()), step.title());
        }
        for pose in Pose::ALL {
            for suffix in ["step-0", "step-1-v2"] {
                let key = format!(
                    "onboarding-stay_aligned-relaxed_poses-{}-{suffix}",
                    pose.name()
                );
                assert_ne!(l10n.text(&key), key);
            }
        }
        for key in [
            "native-stay-aligned-save-error",
            "native-stay-aligned-retry",
            "onboarding-stay_aligned-manual_mounting-done",
        ] {
            assert_ne!(l10n.text(key), key);
        }
    }
}
