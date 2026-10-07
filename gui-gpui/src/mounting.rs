//! Original mounting directions and selection semantics, shared by native pages.
use crate::protocol::{Tracker, TrackerKey};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Body(u8),
    Tracker(TrackerKey),
}
// Clockwise, starting at the top of the original orientation wheel.
pub const DIRECTIONS: &[(&str, f64)] = &[
    ("front", 180.),
    ("front_right", -135.),
    ("right", -90.),
    ("back_right", -45.),
    ("back", 0.),
    ("back_left", 45.),
    ("left", 90.),
    ("front_left", 135.),
];
pub fn quaternion(index: usize) -> [f64; 4] {
    let angle = DIRECTIONS[index].1.to_radians() * 0.5;
    [0., angle.sin(), 0., angle.cos()]
}
pub fn direction(q: [f32; 4]) -> Option<usize> {
    let norm = q.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
    if !norm.is_finite() || norm < 1e-6 {
        return None;
    }
    (0..8).find(|i| {
        q.iter()
            .zip(quaternion(*i))
            .map(|(a, b)| f64::from(*a) / norm * b)
            .sum::<f64>()
            .abs()
            > 0.999
    })
}
pub fn hit(x: f32, y: f32, radius: f32) -> Option<usize> {
    let distance = x.hypot(y);
    if !distance.is_finite() || distance < radius * 0.2 || distance > radius {
        return None;
    }
    let angle = x.atan2(-y).rem_euclid(std::f32::consts::TAU);
    Some(((angle / std::f32::consts::FRAC_PI_4 + 0.5).floor() as usize) % 8)
}
pub fn matches(target: Target, t: &Tracker) -> bool {
    !t.computed
        && t.is_imu
        && t.editable
        && match target {
            Target::Body(body) => body != 0 && t.body == body,
            Target::Tracker(key) => t.key == key,
        }
}
pub fn requests(
    trackers: &[Tracker],
    target: Target,
    index: usize,
) -> Result<Vec<(String, Value)>, String> {
    if index >= DIRECTIONS.len() {
        return Err("Unknown mounting direction".into());
    }
    let [x, y, z, w] = quaternion(index);
    let result: Vec<_> = trackers.iter().filter(|t| matches(target,t)).map(|t| (
        "AssignTrackerRequest".into(), json!({"tracker_id":{"device_id":{"id":t.key.device},"tracker_num":t.key.sensor},"body_position":t.body,"mounting_orientation":{"x":x,"y":y,"z":z,"w":w},"allow_drift_compensation":false})
    )).collect();
    if result.is_empty() {
        Err("No editable IMU tracker remains in this selection".into())
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mounting_a_role_updates_all_its_imus_without_reassigning() {
        let t = |device, body, computed| Tracker {
            key: TrackerKey { device, sensor: 0 },
            body,
            computed,
            editable: true,
            is_imu: true,
            ..Default::default()
        };
        let trackers = [
            t(1, 9, false),
            t(2, 9, false),
            t(3, 10, false),
            t(0, 9, true),
        ];
        let r = requests(&trackers, Target::Body(9), 2).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].1["body_position"], 9);
        assert_eq!(r[1].1["tracker_id"]["device_id"]["id"], 2);
        assert!(r[0].1["display_name"].is_null());
        assert!(requests(&trackers, Target::Body(0), 2).is_err());
        assert!(
            requests(
                &trackers,
                Target::Tracker(TrackerKey {
                    device: 4,
                    sensor: 0
                }),
                2
            )
            .is_err()
        );
    }
    #[test]
    fn wheel_coordinates_match_original_front_and_left_rotations() {
        assert_eq!(hit(0., -100., 160.), Some(0));
        assert_eq!(hit(-100., 0., 160.), Some(6));
        assert_eq!(hit(0., 100., 160.), Some(4));
        assert_eq!(hit(0., 0., 160.), None);
        assert_eq!(hit(170., 0., 160.), None);
        for i in 0..8 {
            let q = quaternion(i).map(|n| n as f32);
            assert_eq!(direction(q), Some(i));
            assert_eq!(direction(q.map(|n| -n)), Some(i));
        }
    }
}
