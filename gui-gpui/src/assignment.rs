//! The original assignment modes progressively expose body parts.
pub fn allowed(mode: &str, part: &str) -> bool {
    if part == "NONE" {
        return true;
    }
    let legs = part.ends_with("UPPER_LEG") || part.ends_with("LOWER_LEG");
    match mode {
        "lower-body" => legs || part == "CHEST",
        "core" => legs || part == "CHEST" || part == "HIP",
        "enhanced-core" => legs || part == "CHEST" || part == "HIP" || part.ends_with("FOOT"),
        "full-body" => {
            legs || part == "CHEST"
                || part == "HIP"
                || part.ends_with("FOOT")
                || part.ends_with("UPPER_ARM")
        }
        _ => true,
    }
}

/// Coordinates from the original PersonFrontIcon's 163 × 392 image.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub body: u8,
    pub x: f32,
    pub y: f32,
    pub left: bool,
    pub label_y: f32,
}
pub fn targets(mode: &str, mirror: bool) -> Vec<Target> {
    use solarxr_protocol::datatypes::BodyPart as B;
    let mut points = Vec::new();
    for (body, x, y, left) in [
        (B::HEAD, 82., 35., true),
        (B::NECK, 82., 80., true),
        (B::UPPER_CHEST, 82., 90., false),
        (B::CHEST, 82., 105., false),
        (B::WAIST, 82., 155., false),
        (B::HIP, 82., 181., true),
    ] {
        if body == B::HEAD || allowed(mode, body.variant_name().unwrap_or("NONE")) {
            points.push(Target {
                body: body.0,
                x,
                y,
                left,
                label_y: (y / 392. - 0.06_f32).max(0.),
            });
        }
    }
    for (l, r, x, y) in [
        (B::LEFT_SHOULDER, B::RIGHT_SHOULDER, 44., 90.),
        (B::LEFT_UPPER_ARM, B::RIGHT_UPPER_ARM, 30., 140.),
        (B::LEFT_LOWER_ARM, B::RIGHT_LOWER_ARM, 20., 185.),
        (B::LEFT_HAND, B::RIGHT_HAND, 15., 207.),
        (B::LEFT_UPPER_LEG, B::RIGHT_UPPER_LEG, 63., 267.),
        (B::LEFT_LOWER_LEG, B::RIGHT_LOWER_LEG, 52., 355.),
        (B::LEFT_FOOT, B::RIGHT_FOOT, 62., 372.),
    ] {
        for (body, draw_left) in [(l, mirror), (r, !mirror)] {
            if body == B::LEFT_HAND
                || body == B::RIGHT_HAND
                || allowed(mode, body.variant_name().unwrap_or("NONE"))
            {
                points.push(Target {
                    body: body.0,
                    x: if draw_left { x } else { 164. - x },
                    y,
                    left: draw_left,
                    label_y: (y / 392. - 0.04_f32).min(0.94),
                });
            }
        }
    }
    position_labels(&mut points, 550.);
    points
}

/// Original BodyAssignment groups, in DOM order. Empty groups also occupy
/// justify-between gaps; legs form one group at the bottom of each side.
pub fn position_labels(points: &mut [Target], height: f32) {
    use solarxr_protocol::datatypes::BodyPart as B;
    for left in [true, false] {
        let groups: Vec<Vec<usize>> = (0..5)
            .map(|group| {
                let mut indices: Vec<_> = (0..points.len())
                    .filter(|&i| {
                        let part = B(points[i].body);
                        let name = part.variant_name().unwrap_or("");
                        let g = if matches!(part, B::HEAD | B::NECK | B::UPPER_CHEST | B::CHEST) {
                            0
                        } else if name.ends_with("SHOULDER") || name.ends_with("UPPER_ARM") {
                            1
                        } else if name.ends_with("LOWER_ARM") || name.ends_with("HAND") {
                            2
                        } else if matches!(part, B::HIP | B::WAIST) {
                            3
                        } else {
                            4
                        };
                        points[i].left == left && g == group
                    })
                    .collect();
                indices.sort_by(|&a, &b| points[a].y.total_cmp(&points[b].y));
                indices
            })
            .collect();
        let used: f32 = groups
            .iter()
            .map(|g| g.len() as f32 * 44. + g.len().saturating_sub(1) as f32 * 8.)
            .sum();
        let gap = ((height - used) / 4.).max(8.);
        let mut y = 0.;
        for group in groups {
            for (rank, i) in group.iter().enumerate() {
                points[*i].label_y = (y + rank as f32 * 52.) / height;
            }
            y += group.len() as f32 * 44. + group.len().saturating_sub(1) as f32 * 8. + gap;
        }
    }
}

pub fn preferred_mode(count: usize) -> &'static str {
    [
        ("lower-body", 5),
        ("core", 6),
        ("enhanced-core", 8),
        ("full-body", 10),
    ]
    .into_iter()
    .find(|(_, limit)| count <= *limit)
    .map(|(mode, _)| mode)
    .unwrap_or("all")
}

/// Both mouse selection and tap setup use the same original replacement rule.
/// Keep each device's mounting orientation when clearing and reassigning it.
pub fn selection_requests(
    trackers: &[crate::protocol::Tracker],
    body: u8,
    selected: Option<crate::protocol::TrackerKey>,
) -> Vec<(String, serde_json::Value)> {
    use serde_json::json;
    let eligible = |t: &&crate::protocol::Tracker| !t.computed && t.editable;
    if body == 0
        || selected.is_some_and(|key| !trackers.iter().filter(eligible).any(|t| t.key == key))
    {
        return Vec::new();
    }
    let request = |t: &crate::protocol::Tracker, role: u8| {
        let mut value = json!({"tracker_id":{"device_id":{"id":t.key.device},"tracker_num":t.key.sensor},"body_position":role,"allow_drift_compensation":false});
        if let Some([x, y, z, w]) = t.mounting {
            value["mounting_orientation"] = json!({"x":x,"y":y,"z":z,"w":w});
        }
        ("AssignTrackerRequest".into(), value)
    };
    let mut requests: Vec<_> = trackers
        .iter()
        .filter(eligible)
        .filter(|t| t.body == body)
        .map(|t| request(t, 0))
        .collect();
    if let Some(t) = selected.and_then(|key| trackers.iter().find(|t| t.key == key)) {
        requests.push(request(t, body));
    }
    requests
}

#[derive(Debug)]
pub struct Warning {
    pub body: u8,
    /// Fluent's original misleadingly named `unassigned` argument holds satisfied bits.
    pub satisfied: u8,
    pub affected: Vec<u8>,
}
pub fn warnings(trackers: &[crate::protocol::Tracker]) -> Vec<Warning> {
    use solarxr_protocol::datatypes::BodyPart as B;
    let roles: std::collections::BTreeSet<_> = trackers
        .iter()
        .filter(|t| !t.computed)
        .map(|t| t.body)
        .collect();
    let spine = vec![B::UPPER_CHEST, B::CHEST, B::WAIST, B::HIP];
    let lower_body = [
        B::LEFT_FOOT,
        B::RIGHT_FOOT,
        B::LEFT_UPPER_LEG,
        B::RIGHT_UPPER_LEG,
        B::LEFT_LOWER_LEG,
        B::RIGHT_LOWER_LEG,
    ];
    let mut result = Vec::new();
    for body in roles.iter().copied() {
        let groups = match B(body) {
            B::LEFT_FOOT => vec![
                vec![B::LEFT_LOWER_LEG],
                vec![B::LEFT_UPPER_LEG],
                spine.clone(),
            ],
            B::RIGHT_FOOT => vec![
                vec![B::RIGHT_LOWER_LEG],
                vec![B::RIGHT_UPPER_LEG],
                spine.clone(),
            ],
            B::LEFT_LOWER_LEG => vec![vec![B::LEFT_UPPER_LEG], spine.clone()],
            B::RIGHT_LOWER_LEG => vec![vec![B::RIGHT_UPPER_LEG], spine.clone()],
            B::LEFT_UPPER_LEG | B::RIGHT_UPPER_LEG => vec![spine.clone()],
            B::HIP | B::WAIST if lower_body.iter().any(|p| roles.contains(&p.0)) => {
                vec![vec![B::CHEST]]
            }
            _ => continue,
        };
        let mut satisfied = 0;
        let mut affected = Vec::new();
        for (i, group) in groups.iter().enumerate() {
            if group.iter().any(|p| roles.contains(&p.0)) {
                satisfied |= 1 << i;
            } else {
                affected.extend(group.iter().map(|p| p.0));
            }
        }
        if !affected.is_empty() {
            result.push(Warning {
                body,
                satisfied,
                affected,
            });
        }
    }
    result
}

/// Shared movement detection for cards, tables and body roles, before a tracker is tapped.
/// Accumulate rotation/acceleration changes for half a second, with bounded history.
#[derive(Default)]
pub struct Motion {
    devices: std::collections::HashMap<crate::protocol::TrackerKey, MotionSample>,
}
#[derive(Default)]
struct MotionSample {
    rotation: Option<[f32; 4]>,
    acceleration: Option<[f32; 3]>,
    changes: std::collections::VecDeque<(std::time::Instant, f32)>,
}
impl Motion {
    pub fn update(&mut self, trackers: &[crate::protocol::Tracker], now: std::time::Instant) {
        self.devices.retain(|key, _| {
            trackers
                .iter()
                .any(|t| t.key == *key && matches!(t.status, 2 | 3))
        });
        self.expire(now);
        for tracker in trackers.iter().filter(|t| matches!(t.status, 2 | 3)) {
            let state = self.devices.entry(tracker.key).or_default();
            let rotation = tracker.raw_rotation.or(tracker.rotation).filter(|q| {
                q.iter().all(|v| v.is_finite()) && q.iter().map(|v| v * v).sum::<f32>() > 0.0001
            });
            let linear = tracker
                .linear_acceleration
                .filter(|a| a.iter().all(|v| v.is_finite()));
            let angular = state
                .rotation
                .zip(rotation)
                .map(|(a, b)| {
                    let dot: f32 = a.iter().zip(b).map(|(a, b)| a * b).sum();
                    let norm: f32 =
                        a.iter().map(|v| v * v).sum::<f32>() * b.iter().map(|v| v * v).sum::<f32>();
                    if norm > 0.0001 {
                        (1. - dot * dot / norm).clamp(0., 1.) * 50.
                    } else {
                        0.
                    }
                })
                .unwrap_or(0.);
            let acceleration = state
                .acceleration
                .zip(linear)
                .map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / 1000.)
                .unwrap_or(0.);
            state
                .changes
                .push_back((now, (angular + acceleration).clamp(0., 1.)));
            while state.changes.front().is_some_and(|(at, _)| {
                now.saturating_duration_since(*at) > std::time::Duration::from_millis(500)
            }) || state.changes.len() > 64
            {
                state.changes.pop_front();
            }
            state.rotation = rotation;
            state.acceleration = linear;
        }
    }
    /// Advance the glow's age using the local UI clock.
    pub fn expire(&mut self, now: std::time::Instant) -> bool {
        let mut changed = false;
        for state in self.devices.values_mut() {
            while state.changes.front().is_some_and(|(at, _)| {
                now.saturating_duration_since(*at) > std::time::Duration::from_millis(500)
            }) {
                changed |= state.changes.pop_front().is_some_and(|(_, v)| v > 0.);
            }
        }
        changed
    }
    pub fn level(&self, key: crate::protocol::TrackerKey) -> f32 {
        self.devices
            .get(&key)
            .map(|s| s.changes.iter().map(|(_, v)| v).sum::<f32>().clamp(0., 1.))
            .unwrap_or(0.)
    }
}
