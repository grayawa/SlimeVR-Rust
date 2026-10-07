//! CPU projection of real 3D coordinates; GPU paths provide the native surface.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: [f32; 2],
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.28,
            pitch: 0.18,
            zoom: 1.0,
            pan: [0.0; 2],
        }
    }
}
impl Camera {
    pub fn rotate(&self, p: [f32; 3]) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let x = p[0] * cy - p[2] * sy;
        let z = p[0] * sy + p[2] * cy;
        [x, p[1] * cp - z * sp, p[1] * sp + z * cp]
    }
    pub fn project(&self, p: [f32; 3], center: [f32; 3], scale: f32) -> [f32; 3] {
        let p = self.rotate([p[0] - center[0], p[1] - center[1], p[2] - center[2]]);
        [
            p[0] * scale * self.zoom + self.pan[0],
            -p[1] * scale * self.zoom + self.pan[1],
            p[2],
        ]
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if dx.is_finite() && dy.is_finite() {
            self.yaw = (self.yaw + dx * 0.01).rem_euclid(std::f32::consts::TAU);
            self.pitch = (self.pitch + dy * 0.01).clamp(-1.55, 1.55);
        }
    }
    pub fn zoom(&mut self, delta: f32) {
        if delta.is_finite() {
            self.zoom = (self.zoom * (delta * 0.01).exp()).clamp(0.25, 4.0);
        }
    }
}
pub fn quaternion_rotate(p: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let n = q.iter().map(|v| v * v).sum::<f32>();
    if !n.is_finite() || n < 1e-8 {
        return p;
    }
    let q = q.map(|v| v / n.sqrt());
    let [x, y, z, w] = q;
    let [u, v, t] = p;
    let a = [
        2.0 * (y * t - z * v),
        2.0 * (z * u - x * t),
        2.0 * (x * v - y * u),
    ];
    [
        u + w * a[0] + y * a[2] - z * a[1],
        v + w * a[1] + z * a[0] - x * a[2],
        t + w * a[2] + x * a[1] - y * a[0],
    ]
}
#[derive(Clone, serde::Deserialize)]
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    pub color: [f32; 3],
}
pub fn mesh(extension: bool) -> &'static [Triangle] {
    static TRACKER: std::sync::LazyLock<Vec<Triangle>> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("tracker_mesh.json")).unwrap()
    });
    static EXTENSION: std::sync::LazyLock<Vec<Triangle>> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("extension_mesh.json")).unwrap()
    });
    if extension { &EXTENSION } else { &TRACKER }
}
