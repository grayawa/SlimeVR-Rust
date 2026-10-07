//! LegTweaks / LegTweaksBuffer with bounded history and an explicit frame clock.
use crate::{
    skeleton::{BodyPosition as B, SkeletonPose},
    Quaternion as Q, Vector3 as V,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
pub const FLOOR_OFFSET: f32 = 0.0025;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct LegConfig {
    pub enabled: bool,
    pub floor_clip: bool,
    pub skating: bool,
    pub toe_snap: bool,
    pub foot_plant: bool,
    pub always_use_floor_clip: bool,
    pub correction_strength: f32,
}
impl Default for LegConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            floor_clip: true,
            skating: true,
            toe_snap: false,
            foot_plant: true,
            always_use_floor_clip: false,
            correction_strength: 0.3,
        }
    }
}
impl LegConfig {
    pub fn validate(self) -> Result<(), String> {
        if !self.correction_strength.is_finite() || !(0.0..=1.0).contains(&self.correction_strength)
        {
            Err("leg correction strength must be 0..1".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FootState {
    Unknown,
    Locked,
    Unlocked,
}
#[derive(Clone, Debug, Serialize)]
pub struct LegFrame {
    pub at_ms: u64,
    pub dt: f32,
    pub feet: [V; 2],
    pub knees: [V; 2],
    pub hip: V,
    pub rotations: [Q; 2],
    pub corrected_feet: [V; 2],
    pub corrected_knees: [V; 2],
    pub corrected_hip: V,
    pub corrected_rotations: [Q; 2],
    pub velocity: [V; 2],
    pub acceleration: [V; 2],
    pub state: [FootState; 2],
    pub numerical: [f32; 2],
    pub com: V,
    pub com_velocity: V,
    pub com_acceleration: V,
    pub standing: bool,
}
impl LegFrame {
    fn seed(at_ms: u64) -> Self {
        Self {
            at_ms,
            dt: 0.0,
            feet: [V::ZERO; 2],
            knees: [V::ZERO; 2],
            hip: V::ZERO,
            rotations: [Q::IDENTITY; 2],
            corrected_feet: [V::ZERO; 2],
            corrected_knees: [V::ZERO; 2],
            corrected_hip: V::ZERO,
            corrected_rotations: [Q::IDENTITY; 2],
            velocity: [V::ZERO; 2],
            acceleration: [V::ZERO; 2],
            state: [FootState::Unknown; 2],
            numerical: [0.0; 2],
            com: V::ZERO,
            com_velocity: V::ZERO,
            com_acceleration: V::ZERO,
            standing: false,
        }
    }
}
pub struct LegTweaks {
    pub config: LegConfig,
    pub floor: f32,
    pub history: VecDeque<LegFrame>,
    initialized: bool,
    invalid: bool,
    hip_floor_dist: f32,
    disengagement: f32,
    unlocked: [u32; 2],
    toe_angle: [f32; 2],
    toe_touched: [bool; 2],
    localizer: bool,
}
impl LegTweaks {
    pub fn new(config: LegConfig) -> Self {
        Self {
            config,
            floor: 0.0,
            history: VecDeque::new(),
            initialized: true,
            invalid: true,
            hip_floor_dist: 0.0,
            disengagement: 0.0,
            unlocked: [0; 2],
            toe_angle: [0.0; 2],
            toe_touched: [false; 2],
            localizer: false,
        }
    }
    pub fn reset_at(&mut self, at_ms: u64, floor: bool) {
        self.reset(floor);
        self.history.push_front(LegFrame::seed(at_ms));
    }
    pub fn reset(&mut self, floor: bool) {
        self.invalid = true;
        self.history.clear();
        self.unlocked = [0; 2];
        self.toe_angle = [0.0; 2];
        self.toe_touched = [false; 2];
        if floor && !self.localizer {
            self.initialized = false;
        }
    }
    pub fn set_localizer(&mut self, enabled: bool) {
        self.localizer = enabled;
        if enabled {
            self.floor = 0.0;
            self.hip_floor_dist = 0.0;
        }
    }
    pub fn frame(&self) -> Option<&LegFrame> {
        self.history.front()
    }
    fn clip(&self, frame: &mut LegFrame, foot_length: f32) {
        let mut total = 0.0;
        for i in 0..2 {
            let floor =
                self.floor + foot_length * foot_offset(frame.rotations[i]) - self.disengagement;
            if frame.corrected_feet[i].y < floor {
                let d = floor - frame.corrected_feet[i].y;
                frame.corrected_feet[i].y += d;
                frame.corrected_knees[i].y += d;
                total += d;
            }
        }
        frame.corrected_hip.y += total / 2.0 * 0.2;
    }
    pub fn update(
        &mut self,
        at_ms: u64,
        pose: &mut SkeletonPose,
        inputs: &BTreeMap<B, Q>,
        accel: &BTreeMap<B, V>,
    ) -> Result<(), String> {
        if !self.config.enabled
            || (!(inputs.contains_key(&B::LeftUpperLeg) && inputs.contains_key(&B::RightUpperLeg))
                && !self.config.always_use_floor_clip)
        {
            return Ok(());
        }
        // Without a world anchor, wait for floor calibration or localization. The
        // upstream zero-height floor initialization otherwise divides by zero.
        if !pose.world_anchor_present
            && !self.localizer
            && self.initialized
            && self.hip_floor_dist == 0.0
        {
            return Ok(());
        }
        let mut f = LegFrame::seed(at_ms);
        f.feet = [
            pose.computed["left_foot"].position,
            pose.computed["right_foot"].position,
        ];
        f.knees = [
            pose.computed["left_knee"].position,
            pose.computed["right_knee"].position,
        ];
        f.hip = pose.computed["hip"].position;
        f.rotations = [
            pose.computed["left_foot"].rotation,
            pose.computed["right_foot"].rotation,
        ];
        f.corrected_feet = f.feet;
        f.corrected_knees = f.knees;
        f.corrected_hip = f.hip;
        f.corrected_rotations = f.rotations;
        if !self.initialized {
            self.floor = (f.feet[0].y + f.feet[1].y) / 2.0 + FLOOR_OFFSET;
            self.hip_floor_dist = f.hip.y - self.floor;
            self.invalid = true;
            self.initialized = true;
        }
        let length = pose.bones["left_foot"].length;
        let cutoff = self.floor + self.hip_floor_dist * 0.35;
        let active = f.hip.y >= cutoff;
        self.disengagement = if active {
            0.0
        } else if self.floor != cutoff {
            (1.0 - (self.floor - f.hip.y) / (self.floor - cutoff)) * 0.3
        } else {
            0.0
        };
        if self.history.is_empty() {
            self.history.push_front(LegFrame::seed(0));
        }
        if self.invalid && !self.localizer {
            if active {
                self.clip(&mut f, length);
            }
            let prev = self.history.front_mut().unwrap();
            prev.feet = f.feet;
            prev.knees = f.knees;
            prev.hip = f.hip;
            prev.corrected_feet = f.corrected_feet;
            prev.corrected_knees = f.corrected_knees;
            prev.corrected_hip = f.corrected_hip;
            prev.state = [FootState::Unlocked; 2];
            f.feet = f.corrected_feet;
            f.knees = f.corrected_knees;
            f.hip = f.corrected_hip;
            self.invalid = false;
        }
        let prev = self.history.front().unwrap().clone();
        f.dt = (at_ms.saturating_sub(prev.at_ms)) as f32 / 1000.0;
        f.com = center_of_mass(pose, inputs) + (f.hip - pose.computed["hip"].position);
        f.com_velocity = f.com - prev.com;
        f.com_acceleration = f.com_velocity - prev.com_velocity;
        let feet_imu = [
            inputs.contains_key(&B::LeftFoot),
            inputs.contains_key(&B::RightFoot),
        ];
        for (i, foot, shin) in [
            (0, B::LeftFoot, B::LeftLowerLeg),
            (1, B::RightFoot, B::RightLowerLeg),
        ] {
            let part = if feet_imu[i] { foot } else { shin };
            f.acceleration[i] = accel.get(&part).copied().unwrap_or(V::ZERO);
            f.velocity[i] = f.feet[i] - prev.feet[i];
        }
        let floors = [
            self.floor + length * foot_offset(f.rotations[0]) - self.disengagement,
            self.floor + length * foot_offset(f.rotations[1]) - self.disengagement,
        ];
        let pressure = pressure_prediction(&f, floors);
        f.standing = pressure.1;
        let threshold_v = 2.4 - 0.3 + self.config.correction_strength;
        let threshold_a = 0.8 - 0.3 + self.config.correction_strength;
        for (i, floor) in floors.iter().enumerate() {
            let diff = (f.velocity[i].len() - f.velocity[1 - i].len()).abs();
            // A newly constructed Kotlin buffer's states are UNKNOWN here, so dormant/locked scalar branches are unreachable.
            let scalar = if diff > 1.75 {
                0.25
            } else if diff < 0.1 {
                3.2
            } else {
                3.2 * (diff - 1.75) / (0.1 - 1.75) - 1.0
            };
            let sv = (1.0 + scalar / 2.0) * (pressure.0[i] * 2.0).clamp(0.1, 1.9);
            let velocity = if f.dt > 0.0 {
                f.velocity[i].len() / f.dt
            } else {
                0.0
            };
            let angular = if f.dt > 0.0 {
                (f.rotations[i].axes()[2] - prev.rotations[i].axes()[2]).len() / f.dt
            } else {
                0.0
            };
            let acceleration_high = f.acceleration[i].len()
                > if feet_imu.iter().all(|v| *v) {
                    threshold_a * 1.1
                } else {
                    threshold_a - 0.1
                };
            let engage = if prev.state[i] == FootState::Unlocked {
                1.1
            } else {
                1.0
            };
            f.state[i] = if !active
                || (f.feet[i] - prev.corrected_feet[i]).horizontal().len() > 0.5 * engage
                || velocity > threshold_v * engage * sv
                || angular > 4.5 * engage * sv
                || f.feet[i].y > *floor + 0.065
                || acceleration_high
            {
                FootState::Unlocked
            } else {
                FootState::Locked
            };
            if active {
                f.numerical[i] = (velocity / (threshold_v * sv)
                    + f.acceleration[i].len() / (threshold_a * scalar))
                    * 0.5;
            }
            self.unlocked[i] = if f.state[i] == FootState::Locked {
                0
            } else {
                self.unlocked[i].saturating_add(1)
            };
        }
        if self.config.foot_plant || self.config.toe_snap {
            for i in 0..2 {
                let angle = (f.corrected_feet[i] - f.corrected_knees[i])
                    .unit()
                    .horizontal()
                    .len();
                let master = if angle < 0.4 {
                    1.0
                } else if angle < 0.7 {
                    1.0 - (angle - 0.4) / 0.3
                } else {
                    0.0
                };
                let mut q = f.rotations[i];
                if self.config.foot_plant {
                    let yaw = Q::rotation_y(q.euler_yzx().y);
                    let mut weight =
                        (1.0 - (f.corrected_feet[i].y - self.floor) / 0.1).clamp(0.0, 1.0);
                    if feet_imu[i] {
                        let a = (q.angle_to_r(yaw) / std::f32::consts::TAU).clamp(0.025, 0.075);
                        weight *= 1.0 - (a - 0.025) / 0.05;
                    }
                    q = q.interp_r(yaw, weight * master);
                }
                if self.config.toe_snap && !feet_imu.iter().all(|v| *v) && length > 0.0 {
                    let height = f.corrected_feet[i].y - self.floor;
                    let angle = (height.clamp(0.0, length) / length).min(0.8).asin();
                    let mut weight = if height > length * 3.0 {
                        0.0
                    } else {
                        (1.0 - (height - length) / (length * 2.0)).clamp(0.0, 1.0)
                    };
                    if !self.toe_touched[i] {
                        weight = weight.min(self.toe_angle[i]);
                    }
                    if !feet_imu[i] {
                        q = q.interp_r(q.replace_pitch_yzx(-angle), weight * master);
                    }
                    if height > length * 0.8 {
                        self.toe_touched[i] = false;
                        self.toe_angle[i] = weight;
                    } else if height <= 0.0 {
                        self.toe_touched[i] = true;
                        self.toe_angle[i] = 1.0;
                    }
                }
                f.corrected_rotations[i] = q;
            }
        }
        if self.config.floor_clip && !self.localizer {
            self.clip(&mut f, length);
        }
        if self.config.skating {
            for i in 0..2 {
                if f.state[i] == FootState::Locked {
                    f.corrected_feet[i].x = prev.corrected_feet[i].x;
                    f.corrected_feet[i].z = prev.corrected_feet[i].z;
                } else {
                    f.corrected_feet[i] = unlocked_foot(
                        f.corrected_feet[i],
                        prev.feet[i],
                        prev.corrected_feet[i],
                        f.velocity[i],
                        self.unlocked[i],
                        self.unlocked[0],
                        f.dt,
                    );
                }
            }
        }
        for i in 0..2 {
            let delta = (f.corrected_feet[i] - f.feet[i]).horizontal() * 0.8;
            f.corrected_knees[i] = f.corrected_knees[i] + delta;
        }
        for (i, side) in [(0, "left"), (1, "right")] {
            let foot = pose.computed.get_mut(&format!("{side}_foot")).unwrap();
            foot.position = f.corrected_feet[i];
            foot.rotation = f.corrected_rotations[i];
            pose.computed
                .get_mut(&format!("{side}_knee"))
                .unwrap()
                .position = f.corrected_knees[i];
        }
        pose.computed.get_mut("hip").unwrap().position = f.corrected_hip;
        if !f
            .corrected_feet
            .iter()
            .chain(f.corrected_knees.iter())
            .all(|v| v.is_finite())
            || !f.corrected_rotations.iter().all(|q| q.is_rotation())
        {
            return Err("non-finite leg correction".into());
        }
        self.history.push_front(f);
        self.history.truncate(11);
        Ok(())
    }
}
fn foot_offset(q: Q) -> f32 {
    q.axes()[2].y.clamp(0.0, 1.0)
}
fn unlocked_foot(
    p: V,
    prev: V,
    corrected: V,
    velocity: V,
    unlocked: u32,
    left_unlocked: u32,
    dt: f32,
) -> V {
    let diff = (p - corrected).horizontal();
    if diff.len() <= 0.001 {
        return p;
    }
    let temp = corrected - (prev - p);
    let mut next = V::new(temp.x, p.y, temp.z);
    let error = (next - corrected).horizontal().len();
    let weight = if error < 0.01 {
        0.55
    } else if error > 0.05 {
        0.70
    } else {
        0.55 + (error - 0.01) / 0.04 * 0.15
    };
    // Preserve upstream's leftFramesUnlocked use in the warmup branch on both sides.
    let constant = if unlocked >= 175 {
        0.5
    } else {
        0.5 * left_unlocked as f32 / 175.0
    };
    for (target, before, after, v, d) in [
        (p.x, corrected.x, &mut next.x, velocity.x, diff.x),
        (p.z, corrected.z, &mut next.z, velocity.z, diff.z),
    ] {
        let c = v * weight + constant * if v > 0.0 { dt } else { -dt };
        if v * d > 0.0 {
            *after += c;
        } else if v * d < 0.0 {
            *after -= c;
        }
        if (target - before) * (target - *after) < 0.0 {
            *after = target;
        }
    }
    next
}
pub fn center_of_mass(p: &SkeletonPose, inputs: &BTreeMap<B, Q>) -> V {
    let mid = |name: &str| {
        let b = p.bones[name];
        (b.head + b.tail) * 0.5
    };
    let mut c = p.bones["head"].head * 0.0827
        + mid("chest") * 0.1870
        + p.bones["waist"].head * 0.1320
        + p.bones["hip"].head * 0.1530;
    for side in ["left", "right"] {
        c = c
            + mid(&format!("{side}_lower_leg")) * 0.0620
            + mid(&format!("{side}_upper_leg")) * 0.1122;
    }
    if (inputs.contains_key(&B::LeftUpperArm) || inputs.contains_key(&B::LeftLowerArm))
        && (inputs.contains_key(&B::RightUpperArm) || inputs.contains_key(&B::RightLowerArm))
    {
        for side in ["left", "right"] {
            c = c
                + mid(&format!("{side}_upper_arm")) * 0.0263
                + mid(&format!("{side}_lower_arm")) * 0.0224;
        }
    } else {
        let location = p.bones["waist"].head + p.bones["upper_chest"].rotation.axes()[2] * 0.15;
        c = c + location * (0.0263 * 2.0) + location * (0.0224 * 2.0);
    }
    p.computed["hip"].position + (c - p.bones["hip_tracker"].head)
}
fn pressure_prediction(f: &LegFrame, floors: [f32; 2]) -> ([f32; 2], bool) {
    let gravity = V::new(0.0, -9.81, 0.0);
    let mut forces = f.feet.map(|p| {
        let v = (p - f.com).unit();
        if v.len() == 0.0 {
            V::ZERO
        } else {
            v * (9.81 * v.y / v.len() / 2.0)
        }
    });
    let error = |a: V, b: V| (f.com_acceleration - (a + b + gravity)).len();
    for _ in 0..100 {
        let e = error(forces[0], forces[1]);
        let l1 = forces[0] * 1.01;
        let l2 = forces[0] * 0.99;
        let r1 = forces[1] * 1.01;
        let r2 = forces[1] * 0.99;
        let e1 = error(l1, forces[1]);
        let e2 = error(l2, forces[1]);
        let e3 = error(r1, forces[0]);
        let e4 = error(r2, forces[0]);
        if e1 < e {
            forces[0] = l1;
        } else if e2 < e {
            forces[0] = l2;
        }
        if e3 < e {
            forces[1] = r1;
        } else if e4 < e {
            forces[1] = r2;
        }
    }
    if (f.com_acceleration - (gravity + forces[0] + forces[1])).len_sq() > 16.0 {
        return ([0.1; 2], false);
    }
    let mut pressure = [0.0; 2];
    for i in 0..2 {
        pressure[i] = forces[i].hadamard(V::new(0.25, 1.0, 0.25)).len()
            / if f.feet[i].y > floors[i] + 0.065 {
                f.feet[i].y - floors[i] - 0.065
            } else {
                0.001
            };
    }
    let sum = pressure[0] + pressure[1];
    if sum == 0.0 {
        ([0.5; 2], true)
    } else {
        ([pressure[0] / sum, pressure[1] / sum], true)
    }
}
