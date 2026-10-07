//! Small dashboard policy, shared by the VR host and desktop preview.
use crate::{
    client::{Connection, Snapshot},
    protocol::ResetKind,
};

pub fn feed_policy(visible: bool) -> (u16, u16, bool) {
    if visible {
        (100, 33, true)
    } else {
        (1000, 1000, false)
    }
}
pub fn reset_allowed(snapshot: &Snapshot, kind: ResetKind) -> bool {
    if snapshot.connection != Connection::Connected
        || snapshot.pending.is_some()
        || snapshot.reset.as_ref().is_some_and(|r| !r.done)
    {
        return false;
    }
    let Some(feed) = &snapshot.feed else {
        return false;
    };
    match kind {
        ResetKind::Full => feed.trackers.iter().any(|t| !t.computed && t.status == 2),
        ResetKind::Yaw => feed.can_yaw,
        ResetKind::Mounting => feed.can_mount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Feed, ResetProgress, Tracker};
    #[test]
    fn reset_requires_connection_capability_and_no_active_operation() {
        let mut s = Snapshot {
            connection: Connection::Connected,
            feed: Some(Feed {
                trackers: vec![Tracker {
                    status: 2,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(reset_allowed(&s, ResetKind::Full));
        assert!(!reset_allowed(&s, ResetKind::Mounting));
        s.feed.as_mut().unwrap().can_mount = true;
        assert!(reset_allowed(&s, ResetKind::Mounting));
        s.reset = Some(ResetProgress {
            tx: 1,
            kind: 0,
            done: false,
            progress_ms: 0,
            duration_ms: 3000,
        });
        assert!(!reset_allowed(&s, ResetKind::Full));
        s.reset.as_mut().unwrap().done = true;
        assert!(reset_allowed(&s, ResetKind::Full));
        s.connection = Connection::Disconnected;
        assert!(!reset_allowed(&s, ResetKind::Full));
    }
    #[test]
    fn hidden_dashboard_disables_bones_and_reduces_telemetry() {
        let hidden = feed_policy(false);
        let shown = feed_policy(true);
        assert!(!hidden.2 && shown.2);
        assert!(hidden.0 > shown.0 && hidden.1 > shown.1);
    }
}
