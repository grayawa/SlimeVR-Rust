//! Match the original TrackersStillOnModal predicate using current telemetry.
use crate::client::{Connection, Snapshot};
use solarxr_protocol::datatypes::TrackerStatus;

pub fn should_warn(enabled: bool, snapshot: &Snapshot) -> bool {
    enabled
        && snapshot.connection == Connection::Connected
        && snapshot.feed.as_ref().is_some_and(|feed| {
            feed.trackers.iter().any(|tracker| {
                tracker.is_imu
                    && tracker.status != TrackerStatus::DISCONNECTED.0
                    && tracker.status != TrackerStatus::TIMED_OUT.0
            })
        })
}
