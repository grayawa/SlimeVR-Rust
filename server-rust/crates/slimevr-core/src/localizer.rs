//! No-HMD root localization from LegTweaks contact and COM history.
use crate::{
    legs::{FootState, LegTweaks, FLOOR_OFFSET},
    skeleton::{BodyPosition as B, SkeletonPose},
    Vector3 as V,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reference {
    Foot,
    Com,
    Sitting,
}
pub struct Localizer {
    pub enabled: bool,
    pub root: V,
    pub reference: Reference,
    target_foot: V,
    target_com: V,
    target_hip: V,
    velocity: V,
    planted: usize,
    warmup: u64,
    foot_frames: u64,
    sitting_frames: u64,
}
impl Localizer {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            root: V::ZERO,
            reference: Reference::Foot,
            target_foot: V::ZERO,
            target_com: V::ZERO,
            target_hip: V::ZERO,
            velocity: V::ZERO,
            planted: 0,
            warmup: 0,
            foot_frames: 0,
            sitting_frames: 0,
        }
    }
    pub fn reset(&mut self) {
        let enabled = self.enabled;
        *self = Self::new(enabled);
    }
    pub fn update(&mut self, legs: &LegTweaks, pose: &SkeletonPose, accel: &BTreeMap<B, V>) {
        if !self.enabled || pose.world_anchor_present {
            return;
        }
        let Some(f) = legs.history.front() else {
            return;
        };
        if legs.history.len() < 2 || f.dt <= 0.0 {
            return;
        }
        let mut acceleration = [B::Waist, B::Hip, B::Chest]
            .iter()
            .find_map(|p| accel.get(p).copied())
            .unwrap_or(V::ZERO);
        if self.warmup < 100 {
            self.velocity = V::ZERO;
            self.target_foot = V::ZERO;
        }
        self.warmup += 1;
        let foot = if f.state[0] == FootState::Locked {
            Some(0)
        } else if f.state[1] == FootState::Locked {
            Some(1)
        } else if f.numerical[0] < f.numerical[1]
            && f.numerical[0] < 50.0
            && f.acceleration[0].y < 2.0
        {
            Some(0)
        } else if f.numerical[1] < f.numerical[0]
            && f.numerical[1] < 50.0
            && f.acceleration[1].y < 2.0
        {
            Some(1)
        } else {
            None
        };
        let foot_travel = if let Some(i) = foot {
            let location = f.feet[i];
            if i != self.planted {
                self.target_foot = location;
                self.planted = i;
            } else if self.reference == Reference::Com {
                self.target_foot = location;
            }
            location - self.target_foot
        } else {
            V::ZERO
        };
        let velocity_y = self.velocity.y;
        let end = legs
            .history
            .iter()
            .find(|h| h.at_ms <= f.at_ms.saturating_sub(100))
            .or_else(|| legs.history.back())
            .unwrap();
        let sample_dt = (f.at_ms - end.at_ms) as f32 / 1000.0;
        if sample_dt > 0.0 {
            self.velocity = (f.com - end.com) / sample_dt;
        }
        if self.foot_frames < 100 {
            acceleration.y = acceleration.y.clamp(-9999.0, 0.0);
        }
        self.velocity.y = velocity_y + (acceleration.y - 2.0) * f.dt;
        if self.reference != Reference::Com {
            self.target_com = f.com;
        }
        self.target_com = self.target_com + self.velocity * f.dt;
        let lowest = pose
            .computed
            .values()
            .map(|p| p.position.y)
            .reduce(f32::min)
            .unwrap_or(0.0);
        if lowest < -FLOOR_OFFSET {
            self.target_com.y += -FLOOR_OFFSET - lowest;
            self.velocity.y = 0.0;
        }
        let com_travel = f.com - self.target_com;
        self.foot_frames = if self.reference == Reference::Foot {
            self.foot_frames + 1
        } else {
            0
        };
        self.sitting_frames = if self.reference == Reference::Sitting {
            self.sitting_frames + 1
        } else {
            0
        };
        let hip = pose.computed["hip"].position;
        let sitting_travel = hip - self.target_hip;
        if lowest < -FLOOR_OFFSET {
            self.target_hip.y += -FLOOR_OFFSET - lowest;
        }
        if self.reference != Reference::Sitting || self.sitting_frames < 1000 {
            self.target_hip = hip;
        }
        let knees = f.knees.map(|k| hip - k);
        let sitting = !f.standing || knees.iter().all(|v| v.y * 1.1 < v.x + v.z);
        self.reference = if sitting {
            Reference::Sitting
        } else if f.feet.iter().all(|p| p.y > 0.0) {
            Reference::Com
        } else {
            Reference::Foot
        };
        let mut travel = if self.reference == Reference::Foot || self.warmup < 100 {
            foot_travel
        } else if self.reference == Reference::Com {
            com_travel
        } else if self.sitting_frames < 1000 {
            foot_travel
        } else {
            sitting_travel
        };
        if self.reference != Reference::Sitting || self.sitting_frames < 1000 {
            travel.y = com_travel.y;
        }
        if travel.is_finite() {
            self.root = self.root - travel;
        }
        // Like the original Localizer, this changes the root for the next FK tick.
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalizerConfig {
    pub enabled: bool,
}
