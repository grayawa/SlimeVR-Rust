//! StayAligned rest locking, relaxed-pose classification and one-tracker gradient descent.
use crate::{skeleton::BodyPosition as B, Quaternion as Q};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
fn wrap(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let r = if a < 0.0 || a >= tau {
        a - (a / tau).floor() * tau
    } else {
        a
    };
    if r > std::f32::consts::PI {
        r - tau
    } else {
        r
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RelaxedPose {
    pub enabled: bool,
    pub upper_leg_degrees: f32,
    pub lower_leg_degrees: f32,
    pub foot_degrees: f32,
}
impl Default for RelaxedPose {
    fn default() -> Self {
        Self {
            enabled: false,
            upper_leg_degrees: 0.0,
            lower_leg_degrees: 0.0,
            foot_degrees: 0.0,
        }
    }
}
impl RelaxedPose {
    pub fn from_rotations(qs: &BTreeMap<B, Q>) -> Self {
        let half = |l, r| match (qs.get(&l), qs.get(&r)) {
            (Some(l), Some(r)) => (wrap(l.euler_yzx().y - r.euler_yzx().y) * 0.5).to_degrees(),
            _ => 0.0,
        };
        Self {
            enabled: true,
            upper_leg_degrees: half(B::LeftUpperLeg, B::RightUpperLeg),
            lower_leg_degrees: half(B::LeftLowerLeg, B::RightLowerLeg),
            foot_degrees: half(B::LeftFoot, B::RightFoot),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AlignmentConfig {
    pub enabled: bool,
    pub hide_correction: bool,
    pub standing: RelaxedPose,
    pub sitting: RelaxedPose,
    pub flat: RelaxedPose,
}
impl AlignmentConfig {
    pub fn validate(self) -> Result<(), String> {
        for p in [self.standing, self.sitting, self.flat] {
            for a in [p.upper_leg_degrees, p.lower_leg_degrees, p.foot_degrees] {
                if !a.is_finite() || a.abs() > 180.0 {
                    return Err("relaxed pose angles must be -180..180 degrees".into());
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestState {
    Moving,
    AtRest,
    RecentlyAtRest,
}
#[derive(Clone, Debug, Serialize)]
pub struct RestDetector {
    pub state: RestState,
    start: u64,
    last: Q,
    last_at: u64,
}
impl RestDetector {
    pub fn new(at: u64) -> Self {
        Self {
            state: RestState::Moving,
            start: at,
            last: Q::IDENTITY,
            last_at: at,
        }
    }
    pub fn update(&mut self, at: u64, q: Q) {
        if self.state == RestState::RecentlyAtRest && at.saturating_sub(self.start) > 3000 {
            self.state = RestState::Moving;
            self.start = at;
            self.last = q;
            self.last_at = at;
        }
        let moved = self.last.angle_to_r(q) > 2f32.to_radians();
        match self.state {
            RestState::Moving | RestState::RecentlyAtRest => {
                if moved {
                    self.last = q;
                    self.last_at = at;
                } else if at.saturating_sub(self.last_at) > 1000 {
                    self.state = RestState::AtRest;
                    self.start = at;
                    self.last = q;
                    self.last_at = at;
                }
            }
            RestState::AtRest => {
                if moved {
                    self.state = RestState::RecentlyAtRest;
                    self.start = at;
                    self.last = q;
                    self.last_at = at;
                }
            }
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct AlignmentState {
    pub rest: RestDetector,
    pub locked_rotation: Option<Q>,
    pub yaw_correction: f32,
    pub errors: [f32; 3],
}
impl AlignmentState {
    fn new(at: u64) -> Self {
        Self {
            rest: RestDetector::new(at),
            locked_rotation: None,
            yaw_correction: 0.0,
            errors: [0.0; 3],
        }
    }
}
pub struct StayAligned {
    pub config: AlignmentConfig,
    pub states: BTreeMap<B, AlignmentState>,
    next: usize,
}
impl StayAligned {
    pub fn new(config: AlignmentConfig) -> Self {
        Self {
            config,
            states: BTreeMap::new(),
            next: 0,
        }
    }
    pub fn reset(&mut self) {
        self.states.clear();
        self.next = 0;
    }
    pub fn reset_body(&mut self, b: B) {
        self.states.remove(&b);
    }
    pub fn correction(&self, b: B) -> Q {
        if !self.config.enabled || self.config.hide_correction {
            Q::IDENTITY
        } else {
            Q::rotation_y(self.states.get(&b).map_or(0.0, |s| s.yaw_correction))
        }
    }
    pub fn update_rest(&mut self, at: u64, b: B, raw: Q, adjusted: Q) {
        let s = self
            .states
            .entry(b)
            .or_insert_with(|| AlignmentState::new(at));
        s.rest.update(at, raw);
        if s.rest.state == RestState::AtRest {
            if s.locked_rotation.is_none() {
                s.locked_rotation = Some(Q::rotation_y(s.yaw_correction) * adjusted);
            }
        } else {
            s.locked_rotation = None;
        }
    }
    pub fn adjust(&mut self, dt: f32, base: &BTreeMap<B, Q>, rates: &BTreeMap<B, f32>) {
        if !self.config.enabled || base.is_empty() {
            return;
        }
        let order = [
            B::Head,
            B::Neck,
            B::UpperChest,
            B::Chest,
            B::Waist,
            B::Hip,
            B::LeftShoulder,
            B::LeftUpperArm,
            B::LeftLowerArm,
            B::LeftHand,
            B::RightShoulder,
            B::RightUpperArm,
            B::RightLowerArm,
            B::RightHand,
            B::LeftUpperLeg,
            B::LeftLowerLeg,
            B::LeftFoot,
            B::RightUpperLeg,
            B::RightLowerLeg,
            B::RightFoot,
        ];
        let bodies: Vec<_> = order.into_iter().filter(|b| base.contains_key(b)).collect();
        if bodies.is_empty() {
            return;
        }
        let b = bodies[self.next % bodies.len()];
        self.next = self.next.wrapping_add(1);
        let Some(rate) = rates.get(&b) else {
            return;
        };
        if *rate <= 0.0 {
            return;
        }
        let Some(s) = self.states.get(&b) else {
            return;
        };
        if s.rest.state == RestState::RecentlyAtRest {
            return;
        }
        let corrected: BTreeMap<_, _> = base
            .iter()
            .map(|(p, q)| {
                (
                    *p,
                    Q::rotation_y(self.states.get(p).map_or(0.0, |s| s.yaw_correction)) * *q,
                )
            })
            .collect();
        let locked = s.locked_rotation;
        let center = center_yaw(&corrected);
        let relaxed = classify(&corrected)
            .and_then(|p| match p {
                0 => Some(self.config.standing),
                1 => Some(self.config.sitting),
                2 => Some(self.config.flat),
                3 => Some(RelaxedPose {
                    enabled: true,
                    ..Default::default()
                }),
                _ => None,
            })
            .filter(|p| p.enabled);
        if s.rest.state == RestState::Moving && (center.is_none() || relaxed.is_none()) {
            return;
        }
        let errors = |q: Q| {
            if let Some(target) = locked {
                return [locked_yaw_error(q, target).abs(), 0.0, 0.0];
            }
            if !has_yaw(q) {
                return [0.0; 3];
            }
            let pose = relaxed.unwrap();
            let yaw = q.euler_yzx().y;
            let offset = relaxed_offset(b, pose);
            let center_error = if b.is_left_arm() || b.is_right_arm() {
                0.0
            } else {
                wrap(center.unwrap() + offset - yaw).abs()
            };
            let mut sum = 0.0;
            for (neighbor, n_offset) in neighbors(b, &corrected, pose) {
                let n = corrected[&neighbor];
                if has_yaw(n) {
                    let e = wrap(n.euler_yzx().y - offset_for_neighbor(n_offset) + offset - yaw);
                    sum += e * e;
                }
            }
            [0.0, center_error, sum.sqrt()]
        };
        let cur = s.yaw_correction;
        let step = rate.to_radians() * dt * bodies.len() as f32;
        let q = corrected[&b];
        let e = errors(q);
        let ep = errors(Q::rotation_y(step) * q);
        let en = errors(Q::rotation_y(-step) * q);
        let score = |v: [f32; 3]| (v[0] - e[0]) * 10.0 + (v[1] - e[1]) * 2.0 + (v[2] - e[2]);
        let (yaw, err) = if score(ep) < 0.0 && score(ep) < score(en) {
            (cur + step, ep)
        } else if score(en) < 0.0 {
            (cur - step, en)
        } else {
            (cur, e)
        };
        let s = self.states.get_mut(&b).unwrap();
        s.yaw_correction = wrap(yaw);
        s.errors = err;
    }
}
fn offset_for_neighbor(v: f32) -> f32 {
    v
}
fn relaxed_offset(b: B, p: RelaxedPose) -> f32 {
    use B::*;
    let (angle, sign) = match b {
        LeftUpperLeg => (p.upper_leg_degrees, 1.0),
        RightUpperLeg => (p.upper_leg_degrees, -1.0),
        LeftLowerLeg => (p.lower_leg_degrees, 1.0),
        RightLowerLeg => (p.lower_leg_degrees, -1.0),
        LeftFoot => (p.foot_degrees, 1.0),
        RightFoot => (p.foot_degrees, -1.0),
        _ => (0.0, 1.0),
    };
    angle.to_radians() * sign
}
fn neighbors(b: B, qs: &BTreeMap<B, Q>, p: RelaxedPose) -> Vec<(B, f32)> {
    use B::*;
    let torso: Vec<_> = [Neck, UpperChest, Chest, Waist, Hip]
        .into_iter()
        .filter(|b| qs.contains_key(b))
        .collect();
    let mut n = Vec::new();
    if b == Head {
        n.extend(torso.first().copied());
    } else if let Some(i) = torso.iter().position(|v| *v == b) {
        if i > 0 {
            n.push(torso[i - 1]);
        }
        if i + 1 < torso.len() {
            n.push(torso[i + 1]);
        } else if qs.contains_key(&LeftUpperLeg) && qs.contains_key(&RightUpperLeg) {
            n.extend([LeftUpperLeg, RightUpperLeg]);
        }
    } else {
        let sides = [
            [LeftUpperLeg, LeftLowerLeg, LeftFoot],
            [RightUpperLeg, RightLowerLeg, RightFoot],
        ];
        for parts in sides {
            if let Some(i) = parts.iter().position(|v| *v == b) {
                if i == 0 {
                    n.extend(torso.last().copied());
                } else {
                    n.push(parts[i - 1]);
                }
                if i < 2 {
                    n.push(parts[i + 1]);
                }
            }
        }
    }
    n.into_iter()
        .filter(|n| qs.contains_key(n))
        .map(|n| (n, relaxed_offset(n, p)))
        .collect()
}
fn has_yaw(q: Q) -> bool {
    q.axes()[0].y.clamp(-1.0, 1.0).acos() > 30f32.to_radians()
}
fn center_yaw(qs: &BTreeMap<B, Q>) -> Option<f32> {
    use B::*;
    let upper: Vec<_> = [Neck, UpperChest, Chest, Waist, Hip]
        .into_iter()
        .filter(|b| qs.contains_key(b))
        .collect();
    if upper.is_empty() {
        return None;
    }
    let legs = [LeftUpperLeg, RightUpperLeg, LeftLowerLeg, RightLowerLeg];
    if legs.iter().any(|b| !qs.contains_key(b) || !has_yaw(qs[b]))
        || upper.iter().any(|b| !has_yaw(qs[b]))
    {
        return None;
    }
    let mut x = 0.0;
    let mut y = 0.0;
    for (b, w) in upper.into_iter().map(|b| (b, 1.0)).chain([
        (Head, 0.5),
        (LeftUpperLeg, 0.4),
        (RightUpperLeg, 0.4),
        (LeftLowerLeg, 0.3),
        (RightLowerLeg, 0.3),
    ]) {
        if let Some(q) = qs.get(&b).filter(|q| has_yaw(**q)) {
            let a = q.euler_yzx().y;
            x += a.cos() * w;
            y += a.sin() * w;
        }
    }
    Some(y.atan2(x))
}
fn tracker_pose(q: Option<&Q>) -> u8 {
    let Some(q) = q else {
        return 0;
    };
    let [x, y, z] = q.axes();
    if x.y.abs() >= y.y.abs() && x.y.abs() >= z.y.abs() {
        5
    } else if y.y.abs() >= x.y.abs() && y.y.abs() >= z.y.abs() {
        if y.y >= 0.0 {
            1
        } else {
            2
        }
    } else if z.y >= 0.0 {
        4
    } else {
        3
    }
}
fn classify(qs: &BTreeMap<B, Q>) -> Option<u8> {
    use B::*;
    let upper: Vec<_> = [Neck, UpperChest, Chest, Waist, Hip]
        .into_iter()
        .filter_map(|b| qs.get(&b))
        .map(|q| tracker_pose(Some(q)))
        .collect();
    let legs = [LeftUpperLeg, RightUpperLeg, LeftLowerLeg, RightLowerLeg]
        .map(|b| tracker_pose(qs.get(&b)));
    if upper.iter().all(|p| *p == 1) && legs == [1, 1, 1, 1] {
        return Some(0);
    }
    let seated = upper.first() == Some(&1) && upper.iter().all(|p| matches!(p, 1 | 3));
    if seated && legs == [3, 3, 1, 1] {
        return Some(1);
    }
    if (seated || upper.iter().all(|p| *p == 3))
        && legs[..2].iter().all(|p| matches!(p, 2 | 3))
        && legs[2..].iter().all(|p| matches!(p, 1 | 3))
    {
        return Some(2);
    }
    if legs[..2].iter().all(|p| matches!(p, 1 | 3)) && legs[2..] == [4, 4] {
        return Some(3);
    }
    None
}
fn locked_yaw_error(q: Q, target: Q) -> f32 {
    let axes = target.axes();
    let i = (0..3)
        .min_by(|a, b| axes[*a].y.abs().total_cmp(&axes[*b].y.abs()))
        .unwrap();
    let a = q.axes()[i];
    let t = axes[i];
    wrap(t.z.atan2(t.x) - a.z.atan2(a.x))
}
/// Degrees per second from StayAlignedDefaults (IMU numeric IDs from FirmwareConstants).
pub fn imu_rate(id: u8, magnetometer: bool) -> f32 {
    if magnetometer {
        return 0.0;
    }
    match id {
        1 => 0.0,
        3 | 4 | 7 | 13 | 15 | 16 | 17 | 19 => 0.15,
        2 | 5 | 6 | 8 | 9 | 12 => 0.4,
        _ => 0.2,
    }
}
