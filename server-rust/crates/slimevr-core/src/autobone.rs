//! Offline AutoBone optimization. Inputs are calibrated, unfiltered motion frames in meters.
//! Uses the upstream normalized objective, bone-contribution steps and simultaneous acceptance.
use crate::{
    skeleton::{BodyPosition as B, HeadPose, Skeleton, SkeletonConfig, SkeletonPose},
    Quaternion as Q, Vector3 as V,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionFrame {
    pub at_ms: u64,
    pub rotations: BTreeMap<B, Q>,
    pub head: HeadPose,
    #[serde(default)]
    pub positions: BTreeMap<B, V>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutoBoneConfig {
    pub epochs: u32,
    pub cursor_increment: usize,
    pub min_distance: usize,
    pub max_distance: usize,
    pub initial_adjust_rate: f32,
    pub adjust_rate_decay: f32,
    pub randomize: bool,
    pub seed: u64,
    pub scale_each_step: bool,
    pub filter_outliers: bool,
    pub calc_initial_error: bool,
    pub use_skeleton_height: bool,
    pub slide_factor: f32,
    pub offset_slide_factor: f32,
    pub foot_height_factor: f32,
    pub proportion_factor: f32,
    pub height_factor: f32,
    pub position_factor: f32,
    pub position_offset_factor: f32,
    pub max_final_error: f32,
}
impl Default for AutoBoneConfig {
    fn default() -> Self {
        Self {
            epochs: 50,
            cursor_increment: 2,
            min_distance: 1,
            max_distance: 1,
            initial_adjust_rate: 10.0,
            adjust_rate_decay: 1.0,
            randomize: true,
            seed: 4,
            scale_each_step: true,
            filter_outliers: false,
            calc_initial_error: false,
            use_skeleton_height: false,
            slide_factor: 1.0,
            offset_slide_factor: 0.0,
            foot_height_factor: 0.0,
            proportion_factor: 0.05,
            height_factor: 0.0,
            position_factor: 0.0,
            position_offset_factor: 0.0,
            max_final_error: 0.03,
        }
    }
}
impl AutoBoneConfig {
    pub fn validate(self) -> Result<(), String> {
        if self.epochs == 0
            || self.epochs > 200
            || self.cursor_increment == 0
            || self.min_distance == 0
            || self.min_distance > self.max_distance
            || self.max_distance > 64
        {
            return Err("invalid AutoBone iteration limits".into());
        }
        for v in [
            self.initial_adjust_rate,
            self.adjust_rate_decay,
            self.slide_factor,
            self.offset_slide_factor,
            self.foot_height_factor,
            self.proportion_factor,
            self.height_factor,
            self.position_factor,
            self.position_offset_factor,
            self.max_final_error,
        ] {
            if !v.is_finite() || !(0.0..=100.0).contains(&v) {
                return Err("AutoBone rates/factors must be finite in 0..100".into());
            }
        }
        if self.slide_factor
            + self.offset_slide_factor
            + self.foot_height_factor
            + self.proportion_factor
            + self.height_factor
            + self.position_factor
            + self.position_offset_factor
            == 0.0
        {
            return Err("AutoBone requires a nonzero objective".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Epoch {
    pub epoch: u32,
    pub mean_error: f32,
    pub standard_deviation: f32,
    pub steps: usize,
    pub skeleton: SkeletonConfig,
}
#[derive(Clone, Debug, Serialize)]
pub struct AutoBoneResult {
    pub skeleton: SkeletonConfig,
    pub target_height: f32,
    pub estimated_height: f32,
    pub initial_error: f32,
    pub final_error: f32,
    pub evaluation_error: f32,
    pub accepted: bool,
    pub epochs: Vec<Epoch>,
    pub frames_used: usize,
}
#[derive(Clone, Copy)]
enum Offset {
    Head,
    Neck,
    UpperChest,
    Chest,
    Waist,
    Hip,
    Hips,
    UpperLeg,
    LowerLeg,
}
const OFFSETS: [Offset; 9] = [
    Offset::Head,
    Offset::Neck,
    Offset::UpperChest,
    Offset::Chest,
    Offset::Waist,
    Offset::Hip,
    Offset::Hips,
    Offset::UpperLeg,
    Offset::LowerLeg,
];
impl Offset {
    fn get(self, c: SkeletonConfig) -> f32 {
        match self {
            Self::Head => c.head_shift,
            Self::Neck => c.neck_length,
            Self::UpperChest => c.upper_chest_length,
            Self::Chest => c.chest_length,
            Self::Waist => c.waist_length,
            Self::Hip => c.hip_length,
            Self::Hips => c.hips_width,
            Self::UpperLeg => c.upper_leg_length,
            Self::LowerLeg => c.lower_leg_length,
        }
    }
    fn set(self, c: &mut SkeletonConfig, v: f32) {
        match self {
            Self::Head => c.head_shift = v,
            Self::Neck => c.neck_length = v,
            Self::UpperChest => c.upper_chest_length = v,
            Self::Chest => c.chest_length = v,
            Self::Waist => c.waist_length = v,
            Self::Hip => c.hip_length = v,
            Self::Hips => c.hips_width = v,
            Self::UpperLeg => c.upper_leg_length = v,
            Self::LowerLeg => c.lower_leg_length = v,
        }
    }
    fn height(self) -> bool {
        !matches!(self, Self::Head | Self::Hips)
    }
    fn bones(self) -> [&'static str; 2] {
        match self {
            Self::Head => ["head"; 2],
            Self::Neck => ["neck"; 2],
            Self::UpperChest => ["upper_chest"; 2],
            Self::Chest => ["chest"; 2],
            Self::Waist => ["waist"; 2],
            Self::Hip => ["hip"; 2],
            Self::Hips => ["left_hip", "right_hip"],
            Self::UpperLeg => ["left_upper_leg", "right_upper_leg"],
            Self::LowerLeg => ["left_lower_leg", "right_lower_leg"],
        }
    }
}
pub fn skeleton_height(c: SkeletonConfig) -> f32 {
    OFFSETS
        .into_iter()
        .filter(|o| o.height())
        .map(|o| o.get(c))
        .sum()
}
fn scale_height(c: &mut SkeletonConfig, target: f32) {
    let factor = target / skeleton_height(*c);
    for o in OFFSETS {
        if o.height() {
            o.set(c, o.get(*c) * factor);
        }
    }
}
fn scale_all(c: &mut SkeletonConfig, f: f32) {
    for o in OFFSETS {
        o.set(c, o.get(*c) * f);
    }
    c.foot_length *= f;
    c.foot_shift *= f;
    c.chest_offset *= f;
    c.hip_offset *= f;
    c.skeleton_offset *= f;
    c.shoulders_distance *= f;
    c.shoulders_width *= f;
    c.upper_arm_length *= f;
    c.lower_arm_length *= f;
    c.hand_y *= f;
    c.hand_z *= f;
    c.elbow_offset *= f;
}
fn solve(c: SkeletonConfig, f: &MotionFrame, height: f32) -> Result<SkeletonPose, String> {
    solve_stateful(&mut Skeleton::new(c), f, height)
}
fn solve_stateful(
    skeleton: &mut Skeleton,
    f: &MotionFrame,
    height: f32,
) -> Result<SkeletonPose, String> {
    let scale = 1.0 / height;
    let head = HeadPose {
        rotation: f.head.rotation,
        position: f.head.position.map(|p| p * scale),
    };
    let positions = f.positions.iter().map(|(b, p)| (*b, *p * scale)).collect();
    skeleton.solve_with_positions(&f.rotations, &positions, Some(head), false)
}
fn feet(p: &SkeletonPose) -> [V; 2] {
    [
        p.bones["left_lower_leg"].tail,
        p.bones["right_lower_leg"].tail,
    ]
}
fn combinations(values: [f32; 4]) -> f32 {
    let mut sum = 0.0;
    for i in 0..4 {
        for j in i + 1..4 {
            sum += (values[i] - values[j]).abs();
        }
    }
    sum / 12.0
}
fn proportion(c: SkeletonConfig) -> f32 {
    let d = SkeletonConfig::default();
    let h = OFFSETS
        .into_iter()
        .filter(|o| o.height())
        .map(|o| o.get(d) as f64)
        .sum::<f64>() as f32;
    let mut sum = 0.0;
    for (value, default, range, scale) in [
        (c.head_shift, d.head_shift, 0.01, false),
        (c.neck_length, d.neck_length, 0.002, true),
        (c.shoulders_width, d.shoulders_width, 0.04, false),
        (c.upper_arm_length, d.upper_arm_length, 0.02, true),
        (c.lower_arm_length, d.lower_arm_length, 0.02, true),
        (c.upper_chest_length, d.upper_chest_length, 0.01, true),
        (c.chest_length, d.chest_length, 0.01, true),
        (c.waist_length, d.waist_length, 0.05, true),
        (c.hip_length, d.hip_length, 0.01, true),
        (c.hips_width, d.hips_width, 0.04, false),
        (c.upper_leg_length, d.upper_leg_length, 0.02, true),
        (c.lower_leg_length, d.lower_leg_length, 0.02, true),
    ] {
        sum += (((if scale { default / h } else { default }) - value).abs() - range).max(0.0);
    }
    sum
}
fn role(b: B) -> Option<&'static str> {
    use B::*;
    match b {
        UpperChest => Some("chest"),
        Hip => Some("hip"),
        LeftUpperLeg => Some("left_knee"),
        RightUpperLeg => Some("right_knee"),
        LeftFoot => Some("left_foot"),
        RightFoot => Some("right_foot"),
        LeftUpperArm => Some("left_elbow"),
        RightUpperArm => Some("right_elbow"),
        LeftHand => Some("left_hand"),
        RightHand => Some("right_hand"),
        _ => None,
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "Paired-frame objective needs two poses and their observations"
)]
fn error(
    c: SkeletonConfig,
    a: &SkeletonPose,
    b: &SkeletonPose,
    fa: &MotionFrame,
    fb: &MotionFrame,
    height: f32,
    target: f32,
    cfg: AutoBoneConfig,
) -> f32 {
    let [l1, r1] = feet(a);
    let [l2, r2] = feet(b);
    let slide = ((l2 - l1).len() + (r2 - r1).len()) / 4.0;
    let offset = combinations([
        (r1 - l1).len(),
        (r2 - l2).len(),
        (r2 - l1).len(),
        (r1 - l2).len(),
    ]);
    let foot_height = combinations([l1.y, r1.y, l2.y, r2.y]);
    let pos_error = |p: &SkeletonPose, f: &MotionFrame| {
        let errors: Vec<_> = f
            .positions
            .iter()
            .filter_map(|(body, pos)| {
                role(*body)
                    .and_then(|r| p.computed.get(r))
                    .map(|r| (*pos - r.position).len())
            })
            .collect();
        if errors.is_empty() {
            0.0
        } else {
            errors.iter().sum::<f32>() / errors.len() as f32
        }
    };
    let mut position_offset = 0.0;
    let mut count = 0;
    for (body, pos1) in &fa.positions {
        if let (Some(pos2), Some(r)) = (fb.positions.get(body), role(*body)) {
            if let (Some(p1), Some(p2)) = (a.computed.get(r), b.computed.get(r)) {
                position_offset +=
                    ((*pos2 - p2.position).len() - (*pos1 - p1.position).len()).abs();
                count += 1;
            }
        }
    }
    if count > 0 {
        position_offset /= count as f32;
    }
    slide * cfg.slide_factor
        + offset * cfg.offset_slide_factor
        + foot_height * cfg.foot_height_factor
        + proportion(c) * cfg.proportion_factor
        + (height - target).abs() * cfg.height_factor
        + (pos_error(a, fa) + pos_error(b, fb)) / 2.0 * cfg.position_factor
        + position_offset * cfg.position_offset_factor
}
fn pair_error(
    c: SkeletonConfig,
    a: &MotionFrame,
    b: &MotionFrame,
    height: f32,
    target: f32,
    cfg: AutoBoneConfig,
) -> Result<(f32, SkeletonPose, SkeletonPose), String> {
    let pa = solve(c, a, height)?;
    let pb = solve(c, b, height)?;
    let e = error(c, &pa, &pb, a, b, height, target, cfg);
    if !e.is_finite() {
        return Err("non-finite AutoBone objective".into());
    }
    Ok((e, pa, pb))
}
#[allow(
    clippy::too_many_arguments,
    reason = "Owned training skeletons retain upstream previous-frame FK state"
)]
fn training_pair_error(
    skeletons: &mut [Skeleton; 2],
    c: SkeletonConfig,
    a: &MotionFrame,
    b: &MotionFrame,
    height: f32,
    target: f32,
    cfg: AutoBoneConfig,
) -> Result<(f32, SkeletonPose, SkeletonPose), String> {
    skeletons[0].config = c;
    skeletons[1].config = c;
    let pa = solve_stateful(&mut skeletons[0], a, height)?;
    let pb = solve_stateful(&mut skeletons[1], b, height)?;
    let e = error(c, &pa, &pb, a, b, height, target, cfg);
    if !e.is_finite() {
        return Err("non-finite AutoBone objective".into());
    }
    Ok((e, pa, pb))
}
fn contribution(o: Offset, a: &SkeletonPose, b: &SkeletonPose, slide: [V; 2]) -> f32 {
    let mut sum = 0.0;
    for (i, name) in o.bones().into_iter().enumerate() {
        let s = slide[i];
        if s.len() <= 0.002 {
            continue;
        }
        let (a, b) = (a.bones[name], b.bones[name]);
        let movement = (b.tail - b.head) - (a.tail - a.head);
        if movement.len() > 0.002 {
            sum += s.unit().dot(movement.unit());
        }
    }
    sum / 2.0
}
/// Kotlin's seeded Random uses Marsaglia's xorwow generator, with 64 warmup draws.
struct KotlinRandom {
    state: [u32; 5],
    addend: u32,
}
impl KotlinRandom {
    fn new(seed: u64) -> Self {
        let a = seed as u32;
        let b = (seed >> 32) as u32;
        let mut r = Self {
            state: [a, b, 0, 0, !a],
            addend: (a << 10) ^ (b >> 4),
        };
        for _ in 0..64 {
            r.next();
        }
        r
    }
    fn next(&mut self) -> u32 {
        let [x, y, z, w, v] = self.state;
        let t = x ^ (x >> 2);
        let t = (t ^ (t << 1)) ^ v ^ (v << 4);
        self.state = [y, z, w, v, t];
        self.addend = self.addend.wrapping_add(362437);
        t.wrapping_add(self.addend)
    }
    fn bounded(&mut self, n: usize) -> usize {
        let n = n as u32;
        if n.is_power_of_two() {
            let bits = n.trailing_zeros();
            let v = self.next();
            return if bits == 0 {
                0
            } else {
                (v >> (32 - bits)) as usize
            };
        }
        loop {
            let bits = self.next() >> 1;
            let v = bits % n;
            if bits.wrapping_sub(v).wrapping_add(n - 1) as i32 >= 0 {
                return v as usize;
            }
        }
    }
}
fn shuffled(n: usize, random: &mut KotlinRandom) -> Vec<usize> {
    let mut indices = vec![0; n];
    let mut zero = usize::MAX;
    for i in 0..n {
        let mut index = random.bounded(n);
        if i > 0 {
            while index == zero || indices[index] > 0 {
                index = random.bounded(n);
            }
        } else {
            zero = index;
        }
        indices[index] = i;
    }
    indices
}
/// Reference helper: the same seeded frame orders used by AutoBone's epochs.
pub fn frame_orders(count: usize, epochs: usize, seed: u64) -> Result<Vec<Vec<usize>>, String> {
    if count == 0 || count > 5000 || epochs > 200 {
        return Err("invalid frame-order limits".into());
    }
    let mut random = KotlinRandom::new(seed);
    Ok((0..epochs).map(|_| shuffled(count, &mut random)).collect())
}
#[derive(Default)]
struct Statistics {
    count: usize,
    mean: f32,
    m2: f32,
}
impl Statistics {
    fn add(&mut self, value: f32) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f32;
        self.m2 += delta * (value - self.mean);
    }
    fn standard_deviation(&self) -> f32 {
        (self.m2 / self.count as f32).sqrt()
    }
}
fn scaled_result(
    initial: SkeletonConfig,
    normalized: SkeletonConfig,
    height: f32,
) -> SkeletonConfig {
    let mut result = initial;
    for offset in OFFSETS {
        if offset.get(initial) > 0.0 {
            offset.set(&mut result, offset.get(normalized) * height);
        }
    }
    result
}
pub fn optimize(
    initial: SkeletonConfig,
    frames: &[MotionFrame],
    target_height: Option<f32>,
    cfg: AutoBoneConfig,
) -> Result<AutoBoneResult, String> {
    optimize_with_progress(initial, frames, target_height, cfg, |_| {})
}
/// Calls the observer after each completed epoch, including the optional initial-error epoch.
pub fn optimize_with_progress(
    initial: SkeletonConfig,
    frames: &[MotionFrame],
    target_height: Option<f32>,
    cfg: AutoBoneConfig,
    mut on_epoch: impl FnMut(&Epoch),
) -> Result<AutoBoneResult, String> {
    cfg.validate()?;
    initial.validate()?;
    if frames.len() < 3 || frames.len() > 5000 {
        return Err("AutoBone requires 3..5000 motion frames".into());
    }
    if frames.windows(2).any(|w| w[1].at_ms <= w[0].at_ms) {
        return Err("AutoBone frame clock must be strictly increasing".into());
    }
    if frames.iter().any(|f| {
        f.rotations.values().any(|q| !q.is_rotation())
            || !f.head.rotation.is_rotation()
            || f.head.position.is_none_or(|p| !p.is_finite())
            || f.positions.values().any(|p| !p.is_finite())
    }) {
        return Err("AutoBone requires finite HMD positions and calibrated rotations".into());
    }
    let target = target_height.unwrap_or_else(|| {
        if cfg.use_skeleton_height {
            return skeleton_height(initial);
        }
        frames
            .iter()
            .map(|f| f.head.position.unwrap().y)
            .reduce(f32::max)
            .unwrap()
    });
    if !target.is_finite() || !(0.4..=3.0).contains(&target) {
        return Err("AutoBone target HMD height must be 0.4..3 meters".into());
    }
    let mut c = initial;
    scale_all(&mut c, 1.0 / skeleton_height(initial));
    let adjusted_height_normalized = skeleton_height(c);
    let adjusted_scale = adjusted_height_normalized / skeleton_height(initial);
    for offset in OFFSETS {
        offset.set(&mut c, offset.get(initial) * adjusted_scale);
    }
    // Training bypasses LegTweaks while preserving the original FK constraint setting.
    let mut used: Vec<&MotionFrame> = frames.iter().collect();
    if cfg.filter_outliers {
        let poses: Vec<_> = used
            .iter()
            .map(|f| solve(c, f, target))
            .collect::<Result<_, _>>()?;
        let mut errors = Vec::new();
        for i in 0..used.len() {
            let mut sum = 0.0;
            for j in 0..used.len() {
                if i != j {
                    sum += error(
                        c, &poses[i], &poses[j], used[i], used[j], target, target, cfg,
                    );
                }
            }
            errors.push(sum / (used.len() - 1) as f32);
        }
        let mean = errors.iter().sum::<f32>() / errors.len() as f32;
        let sd = (errors.iter().map(|e| (e - mean).powi(2)).sum::<f32>() / errors.len() as f32)
            .sqrt()
            * 1.4;
        used = used
            .into_iter()
            .zip(errors)
            .filter(|(_, e)| (*e - mean).abs() <= sd)
            .map(|(f, _)| f)
            .collect();
        if used.len() < 3 {
            return Err("AutoBone outlier filtering left fewer than 3 frames".into());
        }
    }
    let mean_error = |c: SkeletonConfig, height: f32| -> Result<f32, String> {
        let mut sum = 0.0;
        let mut n = 0;
        for distance in cfg.min_distance..=cfg.max_distance.min(used.len() - 1) {
            for i in (0..used.len() - distance).step_by(cfg.cursor_increment) {
                sum += pair_error(c, used[i], used[i + distance], height, target, cfg)?.0;
                n += 1;
            }
        }
        if n == 0 {
            return Err("AutoBone has no usable frame pairs".into());
        }
        Ok(sum / n as f32)
    };
    let initial_error = mean_error(c, target)?;
    let mut height = target;
    let mut epochs = Vec::new();
    let mut random = KotlinRandom::new(cfg.seed);
    let mut statistics = Statistics::default();
    let mut training_skeletons = [Skeleton::new(c), Skeleton::new(c)];
    for skeleton in &mut training_skeletons {
        solve_stateful(skeleton, used[0], target)?;
    }
    for epoch in (if cfg.calc_initial_error { -1 } else { 0 })..cfg.epochs as i32 {
        let order = if cfg.randomize {
            shuffled(used.len(), &mut random)
        } else {
            (0..used.len()).collect()
        };
        let rate = if epoch < 0 {
            0.0
        } else {
            cfg.initial_adjust_rate / (1.0 + cfg.adjust_rate_decay * epoch as f32)
        };
        for distance in cfg.min_distance..=cfg.max_distance.min(used.len() - 1) {
            for i in (0..used.len() - distance).step_by(cfg.cursor_increment) {
                let (a, b) = (used[order[i]], used[order[i + distance]]);
                if !cfg.scale_each_step {
                    let baseline =
                        training_pair_error(&mut training_skeletons, c, a, b, height, target, cfg)?
                            .0;
                    let adjust = 0.5 * (baseline * baseline) * rate;
                    let lo = (height - adjust).clamp(target - 0.2, target + 0.2);
                    let hi = (height + adjust).clamp(target - 0.2, target + 0.2);
                    let el =
                        training_pair_error(&mut training_skeletons, c, a, b, lo, target, cfg)?.0;
                    let eh =
                        training_pair_error(&mut training_skeletons, c, a, b, hi, target, cfg)?.0;
                    if el < baseline && el < eh {
                        height = lo;
                    } else if eh < baseline {
                        height = hi;
                    }
                }
                let (base, pa, pb) =
                    training_pair_error(&mut training_skeletons, c, a, b, height, target, cfg)?;
                statistics.add(base);
                let adjust = 0.5 * (base * base) * rate;
                if adjust == 0.0 {
                    continue;
                }
                let slides = [
                    pb.computed["left_foot"].position - pa.computed["left_foot"].position,
                    pb.computed["right_foot"].position - pa.computed["right_foot"].position,
                ];
                let mut next = c;
                for o in OFFSETS {
                    let original = o.get(c);
                    if original <= 0.0 {
                        continue;
                    }
                    let delta = adjust * -(original * contribution(o, &pa, &pb, slides));
                    if delta == 0.0 {
                        continue;
                    }
                    let candidate = original + delta;
                    if candidate < 0.01 {
                        continue;
                    }
                    let mut trial = c;
                    o.set(&mut trial, candidate);
                    scale_height(&mut trial, 1.0);
                    if training_pair_error(
                        &mut training_skeletons,
                        trial,
                        a,
                        b,
                        height,
                        target,
                        cfg,
                    )?
                    .0 < base
                    {
                        o.set(&mut next, candidate);
                    }
                }
                scale_height(&mut next, adjusted_height_normalized);
                c = next;
            }
        }
        epochs.push(Epoch {
            epoch: (epoch + 1) as u32,
            mean_error: statistics.mean,
            standard_deviation: statistics.standard_deviation(),
            steps: statistics.count,
            skeleton: scaled_result(initial, c, height),
        });
        on_epoch(epochs.last().unwrap());
    }
    let evaluation_error = mean_error(c, height)?;
    // Acceptance and epoch statistics use the upstream cumulative training observations.
    let final_error = statistics.mean;
    let result = scaled_result(initial, c, height);
    result.validate()?;
    Ok(AutoBoneResult {
        skeleton: result,
        target_height: target,
        estimated_height: height,
        initial_error,
        final_error,
        evaluation_error,
        accepted: final_error <= cfg.max_final_error,
        epochs,
        frames_used: used.len(),
    })
}
