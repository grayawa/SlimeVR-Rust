//! Rotational constraints ported from Constraint.kt; angles are radians.
use crate::{Quaternion as Q, Vector3 as V};
#[derive(Clone, Copy, Debug)]
pub enum Constraint {
    Complete,
    TwistSwing { twist: f32, swing: f32 },
    Hinge { min: f32, max: f32, deviation: f32 },
}
fn decompose(q: Q, axis: V) -> (Q, Q) {
    let p = q.project(axis).unit();
    let twist = Q::from_parts(
        (1.0 - p.xyz().len_sq()).max(0.0).sqrt() * if q.w >= 0.0 { 1.0 } else { -1.0 },
        p.xyz(),
    )
    .unit();
    ((q * twist.inv()).unit(), twist)
}
fn cap(q: Q, angle: f32) -> Q {
    let magnitude = (angle * 0.5).sin();
    if q.xyz().len_sq() > magnitude * magnitude {
        Q::from_parts(
            (1.0 - magnitude * magnitude).sqrt() * if q.w >= 0.0 { 1.0 } else { -1.0 },
            q.xyz().unit() * magnitude,
        )
        .unit()
    } else {
        q.unit()
    }
}
fn range(q: Q, min: f32, max: f32, axis: V) -> Q {
    let lo = (min * 0.5).sin();
    let hi = (max * 0.5).sin();
    let lo2 = lo * lo * if min >= 0.0 { 1.0 } else { -1.0 };
    let hi2 = hi * hi * if max >= 0.0 { 1.0 } else { -1.0 };
    let v = q.xyz();
    let magnitude = v.len_sq()
        * if v.dot(axis) * q.w.signum() < 0.0 {
            -1.0
        } else {
            1.0
        };
    if magnitude < lo2 || magnitude > hi2 {
        let dl = (magnitude - lo2).abs().min((magnitude + lo2).abs());
        let dh = (magnitude - hi2).abs().min((magnitude + hi2).abs());
        let (m, m2) = if dl < dh {
            (lo, lo2.abs())
        } else {
            (hi, hi2.abs())
        };
        Q::from_parts((1.0 - m2).max(0.0).sqrt(), v.unit() * -m).unit()
    } else {
        q.unit()
    }
}
impl Constraint {
    pub fn local(self, rotation: Q) -> Q {
        match self {
            Self::Complete => rotation,
            Self::TwistSwing { twist, swing } => {
                let (s, t) = decompose(rotation, V::DOWN);
                cap(s, swing) * cap(t, twist)
            }
            Self::Hinge {
                min,
                max,
                deviation,
            } => {
                let axis = V::new(-1.0, 0.0, 0.0);
                let (s, t) = decompose(rotation, axis);
                cap(s, deviation) * range(t, min, max, axis)
            }
        }
    }
    pub fn world(self, rotation: Q, parent: Q, parent_offset: Q, offset: Q) -> Q {
        if matches!(self, Self::Complete) {
            return rotation;
        }
        let frame = parent * parent_offset.inv() * offset;
        (frame * self.local(frame.inv() * rotation)).unit()
    }
    pub fn for_bone(name: &str) -> Self {
        let ts = |t: f32, s: f32| Self::TwistSwing {
            twist: t.to_radians(),
            swing: s.to_radians(),
        };
        if name.ends_with("_tracker")
            || matches!(name, "head" | "neck")
            || name.ends_with("upper_shoulder")
            || name.contains("proximal")
            || name.contains("intermediate")
            || name.contains("distal")
            || name.contains("metacarpal")
        {
            return Self::Complete;
        }
        match name {
            "upper_chest" => ts(90.0, 120.0),
            "chest" | "waist" | "hip" => ts(60.0, 120.0),
            n if n.ends_with("_hip") => ts(0.0, 15.0),
            n if n.ends_with("upper_leg") || n.ends_with("upper_arm") => ts(120.0, 180.0),
            n if n.ends_with("lower_leg") => Self::Hinge {
                min: 0.0,
                max: 180f32.to_radians(),
                deviation: 50f32.to_radians(),
            },
            n if n.ends_with("lower_arm") => Self::Hinge {
                min: -180f32.to_radians(),
                max: 0.0,
                deviation: 40f32.to_radians(),
            },
            n if n.ends_with("_foot") => ts(60.0, 60.0),
            n if n.ends_with("_hand") => ts(120.0, 120.0),
            n if n.ends_with("_shoulder") => ts(0.0, 30.0),
            _ => Self::Complete,
        }
    }
}
