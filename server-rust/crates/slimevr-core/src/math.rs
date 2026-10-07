//! f32 operations follow the pinned ktmath implementation, including interpQ's branches.
use crate::{Quaternion as Q, Vector3 as V};
use std::ops::{Add, Div, Mul, Neg, Sub};

impl V {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn horizontal(self) -> Self {
        Self { y: 0.0, ..self }
    }
    pub fn hadamard(self, b: Self) -> Self {
        Self::new(self.x * b.x, self.y * b.y, self.z * b.z)
    }
    pub fn cross(self, b: Self) -> Self {
        Self::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UP: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const DOWN: Self = Self {
        x: 0.0,
        y: -1.0,
        z: 0.0,
    };
    pub fn dot(self, b: Self) -> f32 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }
    pub fn len_sq(self) -> f32 {
        self.dot(self)
    }
    pub fn len(self) -> f32 {
        self.len_sq().sqrt()
    }
    pub fn unit(self) -> Self {
        let n = self.len();
        if n == 0.0 {
            Self::ZERO
        } else {
            self / n
        }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}
impl Add for V {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self {
            x: self.x + b.x,
            y: self.y + b.y,
            z: self.z + b.z,
        }
    }
}
impl Sub for V {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        self + -b
    }
}
impl Neg for V {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}
impl Mul<f32> for V {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self {
            x: self.x * b,
            y: self.y * b,
            z: self.z * b,
        }
    }
}
impl Div<f32> for V {
    type Output = Self;
    fn div(self, b: f32) -> Self {
        self * (1.0 / b)
    }
}

impl Q {
    pub const ZERO: Self = Self {
        w: 0.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub fn new(w: f32, x: f32, y: f32, z: f32) -> Self {
        Self { w, x, y, z }
    }
    pub fn xyz(self) -> V {
        V {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
    pub fn from_parts(w: f32, v: V) -> Self {
        Self {
            w,
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
    pub fn dot(self, b: Self) -> f32 {
        self.w * b.w + self.x * b.x + self.y * b.y + self.z * b.z
    }
    pub fn len_sq(self) -> f32 {
        self.dot(self)
    }
    pub fn len(self) -> f32 {
        self.len_sq().sqrt()
    }
    /// Preserve ktmath's zero -> NULL behavior. Validate inputs at the engine boundary.
    pub fn unit(self) -> Self {
        let n = self.len();
        if n == 0.0 {
            Self::ZERO
        } else {
            self / n
        }
    }
    pub fn is_finite(self) -> bool {
        [self.w, self.x, self.y, self.z]
            .iter()
            .all(|x| x.is_finite())
    }
    pub fn is_rotation(self) -> bool {
        self.is_finite() && self.len_sq().is_finite() && self.len_sq() > 0.0
    }
    pub fn inv(self) -> Self {
        let d = self.len_sq();
        Self {
            w: self.w / d,
            x: -self.x / d,
            y: -self.y / d,
            z: -self.z / d,
        }
    }
    pub fn conjugate(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
    pub fn rotation_x(a: f32) -> Self {
        Self::new((a / 2.0).cos(), (a / 2.0).sin(), 0.0, 0.0)
    }
    pub fn rotation_y(a: f32) -> Self {
        Self::new((a / 2.0).cos(), 0.0, (a / 2.0).sin(), 0.0)
    }
    pub fn rotation_z(a: f32) -> Self {
        Self::new((a / 2.0).cos(), 0.0, 0.0, (a / 2.0).sin())
    }
    pub fn from_rotation_vector(v: V) -> Self {
        Self::from_parts(0.0, v / 2.0).exp()
    }
    pub fn from_to(u: V, v: V) -> Self {
        let d = Self::from_parts(0.0, v) / Self::from_parts(0.0, u);
        (d + d.len()).unit()
    }
    pub fn log(self) -> Self {
        let si = self.xyz().len();
        let len = self.len();
        if si == 0.0 {
            Self::from_parts(len.ln(), self.xyz() / self.w)
        } else {
            Self::from_parts(len.ln(), self.xyz() * (si.atan2(self.w) / si))
        }
    }
    pub fn exp(self) -> Self {
        let ang = self.xyz().len();
        let len = self.w.exp();
        if ang == 0.0 {
            Self::from_parts(len, self.xyz() * len)
        } else {
            Self::from_parts(len * ang.cos(), self.xyz() * (len * ang.sin() / ang))
        }
    }
    pub fn pow(self, t: f32) -> Self {
        (self.log() * t).exp()
    }
    pub fn twin_nearest(self, b: Self) -> Self {
        if self.dot(b) < 0.0 {
            -self
        } else {
            self
        }
    }
    pub fn twin_extended_back(self, b: Self) -> Self {
        self.twin_nearest(b * Self::new(0.707, -0.707, 0.0, 0.0))
    }
    pub fn interp_q(self, b: Self, t: f32) -> Self {
        if t == 0.0 {
            self
        } else if t == 1.0 {
            b
        } else if t < 0.5 {
            (b / self).pow(t) * self
        } else {
            (self / b).pow(1.0 - t) * b
        }
    }
    pub fn interp_r(self, b: Self, t: f32) -> Self {
        self.interp_q(b.twin_nearest(self), t)
    }
    pub fn lerp_q(self, b: Self, t: f32) -> Self {
        self * (1.0 - t) + b * t
    }
    pub fn project(self, v: V) -> Self {
        Self::from_parts(self.w, v * (self.xyz().dot(v) / v.len_sq()))
    }
    pub fn yaw_projection(self) -> Self {
        self.project(V::UP).unit()
    }
    pub fn rotate(self, v: V) -> V {
        (self * Self::from_parts(0.0, v) / self).xyz()
    }
    pub fn angle_r(self) -> f32 {
        2.0 * self.xyz().len().atan2(self.w.abs())
    }
    pub fn angle_to_r(self, b: Self) -> f32 {
        (self / b).angle_r()
    }
    /// Column vectors of ktmath's rotation matrix (its matrix fields are column-major).
    pub fn axes(self) -> [V; 3] {
        [
            self.rotate(V::new(1.0, 0.0, 0.0)),
            self.rotate(V::UP),
            self.rotate(V::new(0.0, 0.0, 1.0)),
        ]
    }
    pub fn euler_yzx(self) -> V {
        let [x, y, z] = self.axes();
        let kc = (x.x * x.x + x.z * x.z).sqrt();
        if kc < 1e-7 {
            V::new(
                0.0,
                z.x.atan2(z.z),
                std::f32::consts::FRAC_PI_2.copysign(x.y),
            )
        } else {
            V::new(
                (x.x * y.z - x.z * y.x).atan2(x.x * z.z - x.z * z.x),
                (-x.z).atan2(x.x),
                x.y.atan2(kc),
            )
        }
    }
    /// ktmath ZXY decomposition, including the X = +/- pi/2 singular branch.
    pub fn euler_zxy(self) -> V {
        let [x, y, z] = self.axes();
        let kc = (y.y * y.y + y.x * y.x).sqrt();
        if kc < 1e-7 {
            V::new(
                std::f32::consts::FRAC_PI_2.copysign(y.z),
                0.0,
                x.y.atan2(x.x),
            )
        } else {
            V::new(
                y.z.atan2(kc),
                (y.y * z.x - y.x * z.y).atan2(y.y * x.x - y.x * x.y),
                (-y.x).atan2(y.y),
            )
        }
    }
    pub fn from_euler_yxz(e: V) -> Self {
        Self::rotation_y(e.y) * Self::rotation_x(e.x) * Self::rotation_z(e.z)
    }
    pub fn euler_yxz(self) -> V {
        let [x, y, z] = self.axes();
        let kc = (z.x * z.x + z.z * z.z).sqrt();
        if kc < 1e-7 {
            V::new(
                std::f32::consts::FRAC_PI_2.copysign(-z.y),
                (-x.z).atan2(x.x),
                0.0,
            )
        } else {
            V::new((-z.y).atan2(kc), z.x.atan2(z.z), x.y.atan2(y.y))
        }
    }
    pub fn from_euler_yzx(e: V) -> Self {
        Self::rotation_y(e.y) * Self::rotation_z(e.z) * Self::rotation_x(e.x)
    }
    pub fn replace_pitch_yzx(self, pitch: f32) -> Self {
        Self::from_euler_yzx(V {
            x: pitch,
            ..self.euler_yzx()
        })
    }
    /// Y angle of ktmath's YZX Euler decomposition, including its singular branch.
    pub fn yaw_yzx(self) -> Self {
        let d = self.len_sq();
        let xx = (self.w * self.w + self.x * self.x - self.y * self.y - self.z * self.z) / d;
        let xz = 2.0 * (self.x * self.z - self.w * self.y) / d;
        let zx = 2.0 * (self.w * self.y + self.x * self.z) / d;
        let zz = (self.w * self.w - self.x * self.x - self.y * self.y + self.z * self.z) / d;
        let kc = (xx * xx + xz * xz).sqrt();
        let angle = if kc < 1e-7 {
            zx.atan2(zz)
        } else {
            (-xz).atan2(xx)
        };
        Self::rotation_y(angle).twin_nearest(self)
    }
}
impl Neg for Q {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.w, -self.x, -self.y, -self.z)
    }
}
impl Add for Q {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.w + b.w, self.x + b.x, self.y + b.y, self.z + b.z)
    }
}
impl Add<f32> for Q {
    type Output = Self;
    fn add(self, b: f32) -> Self {
        Self {
            w: self.w + b,
            ..self
        }
    }
}
impl Mul<f32> for Q {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self::new(self.w * b, self.x * b, self.y * b, self.z * b)
    }
}
impl Mul for Q {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        Self::new(
            self.w * b.w - self.x * b.x - self.y * b.y - self.z * b.z,
            self.x * b.w + self.w * b.x - self.z * b.y + self.y * b.z,
            self.y * b.w + self.z * b.x + self.w * b.y - self.x * b.z,
            self.z * b.w - self.y * b.x + self.x * b.y + self.w * b.z,
        )
    }
}
impl Div<f32> for Q {
    type Output = Self;
    fn div(self, b: f32) -> Self {
        self * (1.0 / b)
    }
}
impl Div for Q {
    type Output = Self;
    #[allow(
        clippy::suspicious_arithmetic_impl,
        reason = "Quaternion right division is multiplication by the inverse"
    )]
    fn div(self, b: Self) -> Self {
        self * b.inv()
    }
}
