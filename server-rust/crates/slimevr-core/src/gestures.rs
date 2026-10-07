//! Explicit-clock tap and height calibration state machines.
use crate::{
    skeleton::{BodyPosition as B, HeadPose},
    Quaternion as Q, Vector3 as V,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TapConfig {
    pub enabled: bool,
    pub setup_mode: bool,
    pub yaw_enabled: bool,
    pub full_enabled: bool,
    pub mounting_enabled: bool,
    pub yaw_tracker: Option<B>,
    pub full_tracker: Option<B>,
    pub mounting_tracker: Option<B>,
    pub yaw_taps: u8,
    pub full_taps: u8,
    pub mounting_taps: u8,
    pub max_moving: usize,
    pub yaw_delay_ms: u64,
    pub full_delay_ms: u64,
    pub mounting_delay_ms: u64,
}
impl Default for TapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            setup_mode: false,
            yaw_enabled: true,
            full_enabled: true,
            mounting_enabled: true,
            yaw_tracker: None,
            full_tracker: None,
            mounting_tracker: None,
            yaw_taps: 2,
            full_taps: 3,
            mounting_taps: 3,
            max_moving: 1,
            yaw_delay_ms: 200,
            full_delay_ms: 1000,
            mounting_delay_ms: 1000,
        }
    }
}
impl TapConfig {
    pub fn validate(self) -> Result<(), String> {
        if [self.yaw_taps, self.full_taps, self.mounting_taps]
            .iter()
            .any(|n| !(2..=10).contains(n))
            || self.max_moving > 128
            || [
                self.yaw_delay_ms,
                self.full_delay_ms,
                self.mounting_delay_ms,
            ]
            .iter()
            .any(|v| *v > 60000)
        {
            Err("invalid tap detection limits".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
pub struct TapDetector {
    samples: VecDeque<(u64, f32)>,
    taps: VecDeque<u64>,
    waiting: bool,
}
impl TapDetector {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn update(
        &mut self,
        at: u64,
        body: B,
        accel: &BTreeMap<B, V>,
        count: u8,
        max_moving: usize,
    ) -> bool {
        let Some(a) = accel.get(&body) else {
            return false;
        };
        self.samples.push_back((at, a.len()));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| at.saturating_sub(*t) > 60)
        {
            self.samples.pop_front();
        }
        self.samples.truncate(256);
        let lo = self
            .samples
            .iter()
            .map(|(_, v)| *v)
            .reduce(f32::min)
            .unwrap();
        let hi = self
            .samples
            .iter()
            .map(|(_, v)| *v)
            .reduce(f32::max)
            .unwrap();
        if hi - lo > 6.0 && !self.waiting {
            self.taps.push_back(at);
            self.waiting = true;
        }
        if hi < 2.5 {
            self.waiting = false;
        }
        while self
            .taps
            .front()
            .is_some_and(|t| at.saturating_sub(*t) > 300 * count as u64)
        {
            self.taps.pop_front();
        }
        let moving = [
            B::UpperChest,
            B::Chest,
            B::Hip,
            B::Waist,
            B::LeftUpperLeg,
            B::RightUpperLeg,
            B::LeftFoot,
            B::RightFoot,
        ]
        .into_iter()
        .filter(|b| *b != body && accel.get(b).is_some_and(|a| a.len_sq() > 6.25))
        .count();
        if moving >= max_moving {
            self.reset();
            return false;
        }
        if self.taps.len() >= count as usize {
            self.reset();
            true
        } else {
            false
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeightStatus {
    Idle,
    RecordingFloor,
    WaitingControllerPitch,
    WaitingRise,
    WaitingForwardLook,
    RecordingHeight,
    Done,
    Timeout,
    TooSmall,
    TooHigh,
}
pub struct HeightCalibration {
    pub status: HeightStatus,
    pub height: f32,
    pub floor: f32,
    start: Option<u64>,
    stable: Option<u64>,
    samples: VecDeque<V>,
}
impl Default for HeightCalibration {
    fn default() -> Self {
        Self {
            status: HeightStatus::Idle,
            height: 0.0,
            floor: 0.0,
            start: None,
            stable: None,
            samples: VecDeque::new(),
        }
    }
}
impl HeightCalibration {
    pub fn start(&mut self, at: u64) {
        *self = Self {
            status: HeightStatus::RecordingFloor,
            floor: f32::MAX,
            start: Some(at),
            ..Default::default()
        };
    }
    fn unstable(&mut self) {
        self.stable = None;
        self.samples.clear();
    }
    pub fn update(
        &mut self,
        at: u64,
        head: Option<HeadPose>,
        hands: &BTreeMap<B, (Q, V)>,
    ) -> Option<f32> {
        let start = self.start?;
        if matches!(
            self.status,
            HeightStatus::Done
                | HeightStatus::Timeout
                | HeightStatus::TooSmall
                | HeightStatus::TooHigh
                | HeightStatus::Idle
        ) {
            return None;
        }
        if at.saturating_sub(start) > 30000 {
            self.status = HeightStatus::Timeout;
            return None;
        }
        let (position, tolerance, duration) = match self.status {
            HeightStatus::RecordingFloor | HeightStatus::WaitingControllerPitch => {
                let (q, p) = hands
                    .values()
                    .min_by(|(_, a), (_, b)| a.y.total_cmp(&b.y))?;
                if p.y > 0.1 {
                    self.unstable();
                    return None;
                }
                if q.rotate(V::new(0.0, 0.0, -1.0)).dot(V::DOWN) < 45f32.to_radians().cos() {
                    self.status = HeightStatus::WaitingControllerPitch;
                    self.unstable();
                    return None;
                }
                self.floor = self.floor.min(p.y);
                (*p, 0.005, 300)
            }
            _ => {
                let h = head?;
                let p = h.position?;
                let height = p.y - self.floor;
                if height <= 1.2 {
                    self.status = HeightStatus::WaitingRise;
                    self.unstable();
                    return None;
                }
                self.height = height;
                if h.rotation.rotate(V::UP).dot(V::UP) < 15f32.to_radians().cos() {
                    self.status = HeightStatus::WaitingForwardLook;
                    self.unstable();
                    return None;
                }
                self.status = HeightStatus::RecordingHeight;
                (p, 0.003, 600)
            }
        };
        self.samples.push_back(position);
        if self.samples.len() > 100 {
            self.samples.pop_front();
        }
        if self.samples.len() < 100 {
            return None;
        }
        if position_std_dev(&self.samples) > tolerance {
            self.stable = None;
            return None;
        }
        let stable = *self.stable.get_or_insert(at);
        if at.saturating_sub(stable) < duration {
            return None;
        }
        if matches!(
            self.status,
            HeightStatus::RecordingFloor | HeightStatus::WaitingControllerPitch
        ) {
            self.status = HeightStatus::WaitingRise;
            self.unstable();
            None
        } else {
            self.status = if self.height < 1.2 {
                HeightStatus::TooSmall
            } else if self.height > 1.936 {
                HeightStatus::TooHigh
            } else {
                HeightStatus::Done
            };
            if self.status == HeightStatus::Done {
                Some(self.height)
            } else {
                None
            }
        }
    }
}
pub fn position_std_dev(samples: &VecDeque<V>) -> f32 {
    if samples.is_empty() {
        return f32::MAX;
    }
    let sum = samples.iter().fold(V::ZERO, |a, b| a + *b);
    let mean = sum / samples.len() as f32;
    (samples.iter().map(|v| (*v - mean).len_sq()).sum::<f32>() / samples.len() as f32).sqrt()
}
