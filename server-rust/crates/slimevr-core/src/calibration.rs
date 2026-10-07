//! TrackerResetsHandler rotation resets, including arm/finger mounting modes. No IMU-side calibration.
use crate::{Quaternion as Q, Vector3 as V};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetKind {
    Full,
    Yaw,
    Mounting,
}

/// External HMDs retain world heading/roll; Full reset optionally removes pitch.
#[derive(Default)]
pub struct HmdCalibration {
    pitch_fix: Option<Q>,
}
impl HmdCalibration {
    pub fn reset_full(&mut self, raw: Q, enabled: bool) -> Result<(), String> {
        let fix = enabled.then(|| {
            let without_yaw = raw.yaw_yzx().inv() * raw;
            Q::new(without_yaw.w, -without_yaw.x, 0.0, 0.0).unit()
        });
        if fix.is_some_and(|q| !q.is_rotation()) {
            return Err("HMD pitch reset has a degenerate pitch projection".into());
        }
        self.pitch_fix = fix;
        Ok(())
    }
    pub fn adjust(&self, raw: Q) -> Q {
        raw * self.pitch_fix.unwrap_or(Q::IDENTITY)
    }
    pub fn clear(&mut self) {
        self.pitch_fix = None;
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Calibration {
    pub mounting: Q,
    pub gyro_fix: Q,
    pub attachment_fix: Q,
    pub mount_rot_fix: Q,
    pub yaw_fix: Q,
    pub constraint_fix: Q,
    pub tpose_down_fix: Q,
    pub full_reset_done: bool,
    pub mounting_reset_done: bool,
    transition_start: Q,
    transition_left: f32,
    transition_total: f32,
    pub yaw_transition: Q,
}
impl Calibration {
    pub fn new(mounting: Q) -> Self {
        Self {
            mounting,
            gyro_fix: Q::IDENTITY,
            attachment_fix: Q::IDENTITY,
            mount_rot_fix: Q::IDENTITY,
            yaw_fix: Q::IDENTITY,
            constraint_fix: Q::IDENTITY,
            tpose_down_fix: Q::IDENTITY,
            full_reset_done: false,
            mounting_reset_done: false,
            transition_start: Q::IDENTITY,
            transition_left: 0.0,
            transition_total: 0.0,
            yaw_transition: Q::IDENTITY,
        }
    }
    pub fn adjust(&self, raw: Q) -> Q {
        let rot = self.gyro_fix * (raw * self.mounting) * self.attachment_fix;
        self.constraint_fix
            * (self.yaw_fix
                * (self.mount_rot_fix.inv() * (rot * self.mount_rot_fix) * self.tpose_down_fix))
    }
    fn fix_yaw(&self, raw_mounted: Q, reference: Q) -> Q {
        let rot = self.gyro_fix * raw_mounted * self.attachment_fix;
        let rot = self.mount_rot_fix.inv() * (rot * self.mount_rot_fix);
        rot.yaw_yzx().inv() * reference.yaw_projection()
    }
    pub fn reset_full(&mut self, raw: Q, reference: Q) {
        self.reset_full_body(
            raw,
            reference,
            crate::skeleton::BodyPosition::Chest,
            ArmsResetMode::Back,
        );
    }
    pub fn reset_full_body(
        &mut self,
        raw: Q,
        reference: Q,
        body: crate::skeleton::BodyPosition,
        mode: ArmsResetMode,
    ) {
        self.constraint_fix = Q::IDENTITY;
        self.tpose_down_fix = if mode == ArmsResetMode::TposeDown {
            if body.is_left_arm() || body.is_left_finger() {
                Q::rotation_z(-std::f32::consts::FRAC_PI_2)
            } else if body.is_right_arm() || body.is_right_finger() {
                Q::rotation_z(std::f32::consts::FRAC_PI_2)
            } else {
                Q::IDENTITY
            }
        } else {
            Q::IDENTITY
        };
        let mounted = raw * self.mounting;
        self.gyro_fix = (mounted * self.tpose_down_fix).yaw_yzx().inv();
        self.attachment_fix = (self.gyro_fix * mounted).inv();
        if self.tpose_down_fix != Q::IDENTITY {
            self.attachment_fix = self.attachment_fix * Q::rotation_y(std::f32::consts::PI);
        }
        self.yaw_fix = self.fix_yaw(mounted, reference);
        self.clear_transition();
        self.full_reset_done = true;
    }
    /// Positional/computed trackers use source mounting and retain their own heading frame.
    pub fn reset_full_computed(
        &mut self,
        raw: Q,
        reference: Q,
        body: crate::skeleton::BodyPosition,
        mode: ArmsResetMode,
    ) {
        use crate::skeleton::BodyPosition as B;
        self.constraint_fix = Q::IDENTITY;
        self.tpose_down_fix = if mode == ArmsResetMode::TposeDown {
            if body.is_left_arm() || body.is_left_finger() {
                Q::rotation_z(-std::f32::consts::FRAC_PI_2)
            } else if body.is_right_arm() || body.is_right_finger() {
                Q::rotation_z(std::f32::consts::FRAC_PI_2)
            } else {
                Q::IDENTITY
            }
        } else {
            Q::IDENTITY
        };
        if body == B::Head {
            self.gyro_fix = raw.yaw_yzx().inv();
        } else {
            self.mount_rot_fix = reference.yaw_yzx();
        }
        self.attachment_fix = (self.gyro_fix * raw).inv();
        if body != B::Head {
            self.yaw_fix = self.fix_yaw(raw, reference);
            self.clear_transition();
        }
        self.full_reset_done = true;
    }
    pub fn reset_yaw(&mut self, raw: Q, reference: Q, seconds: f32) {
        self.constraint_fix = Q::IDENTITY;
        let old = self.yaw_fix;
        self.yaw_fix = self.fix_yaw(raw * self.mounting, reference);
        self.clear_transition();
        if seconds > 0.0 {
            self.transition_start = (old / self.yaw_fix).twin_nearest(Q::IDENTITY);
            self.transition_total = seconds;
            self.transition_left = seconds;
            self.yaw_transition = self.transition_start;
        }
    }
    /// Caller must supply the prescribed mounting pose. Thighs use the forward branch.
    pub fn reset_mounting(&mut self, raw: Q, reference: Q, is_thigh: bool) {
        self.constraint_fix = Q::IDENTITY;
        let rot = self.yaw_fix * (self.gyro_fix * (raw * self.mounting) * self.attachment_fix);
        let rot = reference.project(V::UP).inv().unit() * rot;
        let up = rot.rotate(V::UP);
        let mut angle = up.x.atan2(up.z);
        if !is_thigh {
            angle -= std::f32::consts::PI;
        }
        self.mount_rot_fix = Q::rotation_y(angle);
        self.mounting_reset_done = true;
    }
    pub fn reset_mounting_body(
        &mut self,
        raw: Q,
        reference: Q,
        body: crate::skeleton::BodyPosition,
        mode: ArmsResetMode,
    ) {
        use crate::skeleton::BodyPosition as B;
        self.constraint_fix = Q::IDENTITY;
        let rot = reference.project(V::UP).inv().unit()
            * (self.yaw_fix * (self.gyro_fix * (raw * self.mounting) * self.attachment_fix));
        let up = rot.rotate(V::UP);
        let mut angle = up.x.atan2(up.z);
        if (body.is_left_arm() && mode == ArmsResetMode::TposeDown)
            || (body.is_right_arm() && mode == ArmsResetMode::TposeUp)
            || body.is_left_finger()
        {
            angle -= std::f32::consts::FRAC_PI_2;
        }
        if (body.is_left_arm() && mode == ArmsResetMode::TposeUp)
            || (body.is_right_arm() && mode == ArmsResetMode::TposeDown)
            || body.is_right_finger()
        {
            angle += std::f32::consts::FRAC_PI_2;
        }
        let lower_back = mode == ArmsResetMode::Back
            && matches!(
                body,
                B::LeftLowerArm | B::RightLowerArm | B::LeftHand | B::RightHand
            );
        let forward = mode == ArmsResetMode::Forward && (body.is_left_arm() || body.is_right_arm());
        if !body.is_thigh() && !lower_back && !forward {
            angle -= std::f32::consts::PI;
        }
        self.mount_rot_fix = Q::rotation_y(angle);
        self.mounting_reset_done = true;
    }
    fn clear_transition(&mut self) {
        self.transition_start = Q::IDENTITY;
        self.transition_left = 0.0;
        self.transition_total = 0.0;
        self.yaw_transition = Q::IDENTITY;
    }
    pub fn tick(&mut self, dt: f32) {
        if self.transition_left > 0.0 {
            self.transition_left -= dt;
            if self.transition_left > 0.0 {
                let t = self.transition_left / self.transition_total;
                let eased = (3.0 - 2.0 * t) * t * t;
                self.yaw_transition = Q::IDENTITY.interp_r(self.transition_start, eased);
            } else {
                self.transition_left = 0.0;
                self.yaw_transition = Q::IDENTITY;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmsResetMode {
    #[default]
    Back,
    Forward,
    TposeUp,
    TposeDown,
}
