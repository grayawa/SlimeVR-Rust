//! Resistance/angle flex sensors, including reversed resistance and finger-specific axes.
use crate::{skeleton::BodyPosition as B, Quaternion as Q, Vector3 as V};
#[derive(Default)]
pub struct FlexSensor {
    min: Option<f32>,
    max: Option<f32>,
    last_min: f32,
    last: f32,
    reversed: bool,
}
impl FlexSensor {
    pub fn angle(body: B, angle: f32) -> Q {
        let thumb = matches!(
            body,
            B::LeftThumbMetacarpal
                | B::LeftThumbProximal
                | B::LeftThumbDistal
                | B::RightThumbMetacarpal
                | B::RightThumbProximal
                | B::RightThumbDistal
        );
        let e = if thumb {
            let s = if body.is_left_finger() { 1.0 } else { -1.0 };
            V::new(
                std::f32::consts::PI / 8.0 - angle,
                -s * angle * 0.05,
                s * angle * 0.1,
            )
        } else if body.is_left_finger() || body == B::RightShoulder {
            V::new(0.0, 0.0, angle)
        } else if body.is_right_finger() || body == B::LeftShoulder {
            V::new(0.0, 0.0, -angle)
        } else {
            V::new(angle, 0.0, 0.0)
        };
        Q::from_euler_yzx(e)
    }
    pub fn resistance(&mut self, body: B, value: f32) -> Result<Q, String> {
        if !value.is_finite() {
            return Err("non-finite flex resistance".into());
        }
        self.min = Some(self.min.map_or(value, |v| {
            if self.reversed {
                v.max(value)
            } else {
                v.min(value)
            }
        }));
        self.max = Some(self.max.map_or(value, |v| {
            if self.reversed {
                v.min(value)
            } else {
                v.max(value)
            }
        }));
        let lo = self.min.unwrap();
        let hi = self.max.unwrap();
        self.last = value;
        let bend = if lo == hi {
            0.0
        } else {
            max_bend(body) * (value - lo) / (hi - lo)
        };
        Ok(Self::angle(body, bend))
    }
    pub fn reset_min(&mut self, body: B) -> Q {
        self.min = Some(self.last);
        self.last_min = self.last;
        self.resistance(body, self.last).unwrap()
    }
    pub fn reset_max(&mut self, body: B) -> Q {
        let reversed = self.last < self.last_min;
        if reversed != self.reversed {
            self.reversed = reversed;
            self.min = self.max;
            self.max = Some(self.last_min);
        } else {
            self.max = Some(self.last);
        }
        self.resistance(body, self.last).unwrap()
    }
}
fn max_bend(b: B) -> f32 {
    use B::*;
    let units = match b {
        LeftThumbMetacarpal | RightThumbMetacarpal => 0.375,
        LeftThumbProximal | RightThumbProximal => 0.625,
        LeftThumbDistal | RightThumbDistal => 1.125,
        LeftShoulder | RightShoulder => 0.25,
        LeftIndexProximal | LeftMiddleProximal | LeftRingProximal | LeftLittleProximal
        | RightIndexProximal | RightMiddleProximal | RightRingProximal | RightLittleProximal => 0.5,
        LeftIndexIntermediate
        | LeftMiddleIntermediate
        | LeftRingIntermediate
        | LeftLittleIntermediate
        | RightIndexIntermediate
        | RightMiddleIntermediate
        | RightRingIntermediate
        | RightLittleIntermediate => 1.0,
        LeftIndexDistal | LeftMiddleDistal | LeftRingDistal | LeftLittleDistal
        | RightIndexDistal | RightMiddleDistal | RightRingDistal | RightLittleDistal => 1.5,
        _ => 0.75,
    };
    units * std::f32::consts::PI
}
