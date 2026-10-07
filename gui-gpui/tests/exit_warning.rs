use slimevr_gpui::{
    client::{Connection, Snapshot},
    exit_warning::should_warn,
    protocol::{Feed, Tracker, TrackerKey},
};
use solarxr_protocol::datatypes::TrackerStatus as S;

fn snapshot(trackers: Vec<Tracker>) -> Snapshot {
    Snapshot {
        connection: Connection::Connected,
        feed: Some(Feed {
            trackers,
            bones: vec![],
            can_yaw: false,
            can_mount: false,
            can_height: false,
        }),
        ..Default::default()
    }
}
fn tracker(status: S, is_imu: bool) -> Tracker {
    Tracker {
        key: TrackerKey {
            device: 1,
            sensor: 0,
        },
        status: status.0,
        is_imu,
        ..Default::default()
    }
}

#[test]
fn original_live_imu_statuses_warn_including_busy_and_error() {
    for status in [S::NONE, S::OK, S::BUSY, S::ERROR, S::OCCLUDED] {
        assert!(should_warn(true, &snapshot(vec![tracker(status, true)])));
    }
    for status in [S::DISCONNECTED, S::TIMED_OUT] {
        assert!(!should_warn(true, &snapshot(vec![tracker(status, true)])));
    }
}

#[test]
fn hmd_controllers_and_computed_nodes_do_not_trigger_the_imu_warning() {
    let mut computed = tracker(S::OK, false);
    computed.key.device = 0;
    assert!(!should_warn(
        true,
        &snapshot(vec![tracker(S::OK, false), computed.clone()])
    ));
    assert!(should_warn(
        true,
        &snapshot(vec![computed, tracker(S::ERROR, true)])
    ));
}

#[test]
fn disabled_warning_and_missing_or_stale_telemetry_allow_exit() {
    let mut live = snapshot(vec![tracker(S::OK, true)]);
    assert!(!should_warn(false, &live));
    for connection in [Connection::Connecting, Connection::Disconnected] {
        live.connection = connection;
        assert!(!should_warn(true, &live));
    }
    assert!(!should_warn(true, &snapshot(vec![])));
    assert!(!should_warn(
        true,
        &Snapshot {
            connection: Connection::Connected,
            ..Default::default()
        }
    ));
}
