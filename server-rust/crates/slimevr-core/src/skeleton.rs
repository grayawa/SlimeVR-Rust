//! HumanSkeleton body/arm/finger assignments, constraints and Bone/TransformNode FK.
use crate::{Quaternion as Q, Vector3 as V};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyPosition {
    Head,
    Neck,
    UpperChest,
    Chest,
    Waist,
    Hip,
    LeftUpperLeg,
    RightUpperLeg,
    LeftLowerLeg,
    RightLowerLeg,
    LeftFoot,
    RightFoot,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    LeftThumbMetacarpal,
    LeftThumbProximal,
    LeftThumbDistal,
    LeftIndexProximal,
    LeftIndexIntermediate,
    LeftIndexDistal,
    LeftMiddleProximal,
    LeftMiddleIntermediate,
    LeftMiddleDistal,
    LeftRingProximal,
    LeftRingIntermediate,
    LeftRingDistal,
    LeftLittleProximal,
    LeftLittleIntermediate,
    LeftLittleDistal,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
    RightThumbMetacarpal,
    RightThumbProximal,
    RightThumbDistal,
    RightIndexProximal,
    RightIndexIntermediate,
    RightIndexDistal,
    RightMiddleProximal,
    RightMiddleIntermediate,
    RightMiddleDistal,
    RightRingProximal,
    RightRingIntermediate,
    RightRingDistal,
    RightLittleProximal,
    RightLittleIntermediate,
    RightLittleDistal,
}
impl BodyPosition {
    pub fn is_left_arm(self) -> bool {
        matches!(
            self,
            Self::LeftShoulder | Self::LeftUpperArm | Self::LeftLowerArm | Self::LeftHand
        )
    }
    pub fn is_right_arm(self) -> bool {
        matches!(
            self,
            Self::RightShoulder | Self::RightUpperArm | Self::RightLowerArm | Self::RightHand
        )
    }
    pub fn is_left_finger(self) -> bool {
        self >= Self::LeftThumbMetacarpal && self <= Self::LeftLittleDistal
    }
    pub fn is_right_finger(self) -> bool {
        self >= Self::RightThumbMetacarpal && self <= Self::RightLittleDistal
    }

    pub fn is_thigh(self) -> bool {
        matches!(self, Self::LeftUpperLeg | Self::RightUpperLeg)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SkeletonConfig {
    pub head_shift: f32,
    pub neck_length: f32,
    pub upper_chest_length: f32,
    pub chest_length: f32,
    pub waist_length: f32,
    pub hip_length: f32,
    pub hips_width: f32,
    pub upper_leg_length: f32,
    pub lower_leg_length: f32,
    pub foot_length: f32,
    pub foot_shift: f32,
    pub chest_offset: f32,
    pub hip_offset: f32,
    pub skeleton_offset: f32,
    pub shoulders_distance: f32,
    pub shoulders_width: f32,
    pub upper_arm_length: f32,
    pub lower_arm_length: f32,
    pub hand_y: f32,
    pub hand_z: f32,
    pub elbow_offset: f32,
    pub force_arms_from_hmd: bool,
    pub enforce_constraints: bool,
    /// Compatibility settings retained by the configuration model. Regular pose updates
    /// use FK with the configured constraints and fixed feedback policy.
    pub use_position: bool,
    pub correct_constraints: bool,
    pub extended_spine: bool,
    pub extended_pelvis: bool,
    pub extended_knee: bool,
    pub waist_from_chest_hip: f32,
    pub waist_from_chest_legs: f32,
    pub hip_from_chest_legs: f32,
    pub hip_from_waist_legs: f32,
    pub hip_legs: f32,
    pub knee_tracker_ankle: f32,
    pub knee_ankle: f32,
}
impl Default for SkeletonConfig {
    fn default() -> Self {
        Self {
            head_shift: 0.1,
            neck_length: 0.1,
            upper_chest_length: 0.16,
            chest_length: 0.16,
            waist_length: 0.20,
            hip_length: 0.04,
            hips_width: 0.26,
            upper_leg_length: 0.42,
            lower_leg_length: 0.50,
            foot_length: 0.05,
            foot_shift: -0.05,
            chest_offset: 0.0,
            hip_offset: 0.0,
            skeleton_offset: 0.0,
            shoulders_distance: 0.08,
            shoulders_width: 0.35,
            upper_arm_length: 0.26,
            lower_arm_length: 0.26,
            hand_y: 0.035,
            hand_z: 0.13,
            elbow_offset: 0.0,
            force_arms_from_hmd: true,
            enforce_constraints: true,
            use_position: true,
            correct_constraints: false,
            extended_spine: true,
            extended_pelvis: true,
            extended_knee: true,
            waist_from_chest_hip: 0.30,
            waist_from_chest_legs: 0.30,
            hip_from_chest_legs: 0.50,
            hip_from_waist_legs: 0.40,
            hip_legs: 0.25,
            knee_tracker_ankle: 0.85,
            knee_ankle: 0.0,
        }
    }
}
impl SkeletonConfig {
    /// SkeletonConfigManager.resetOffsets, with BodyProportionError's height-scaled limiters.
    pub fn reset_offsets_for_height(mut self, height: f32) -> Result<Self, String> {
        if !height.is_finite() || !(1.2..=1.936).contains(&height) {
            return Err("HMD height must be in 1.2..1.936 meters".into());
        }
        let d = Self::default();
        let default_height = d.neck_length
            + d.upper_chest_length
            + d.chest_length
            + d.waist_length
            + d.hip_length
            + d.upper_leg_length
            + d.lower_leg_length;
        // Preserve model toggles/ratios; resetting offsets replaces all offset values.
        self.head_shift = d.head_shift;
        self.neck_length = height * (d.neck_length / default_height);
        self.upper_chest_length = height * (d.upper_chest_length / default_height);
        self.chest_length = height * (d.chest_length / default_height);
        self.waist_length = height * (d.waist_length / default_height);
        self.hip_length = height * (d.hip_length / default_height);
        self.upper_leg_length = height * (d.upper_leg_length / default_height);
        self.lower_leg_length = height * (d.lower_leg_length / default_height);
        self.upper_arm_length = height * (d.upper_arm_length / default_height);
        self.lower_arm_length = height * (d.lower_arm_length / default_height);
        self.hips_width = d.hips_width;
        self.shoulders_width = d.shoulders_width;
        self.shoulders_distance = d.shoulders_distance;
        self.foot_length = d.foot_length;
        self.foot_shift = d.foot_shift;
        self.chest_offset = d.chest_offset;
        self.hip_offset = d.hip_offset;
        self.skeleton_offset = d.skeleton_offset;
        self.hand_y = d.hand_y;
        self.hand_z = d.hand_z;
        self.elbow_offset = d.elbow_offset;
        self.validate()?;
        Ok(self)
    }
    pub fn validate(self) -> Result<(), String> {
        let lengths = [
            self.neck_length,
            self.upper_chest_length,
            self.chest_length,
            self.waist_length,
            self.hip_length,
            self.hips_width,
            self.upper_leg_length,
            self.lower_leg_length,
            self.foot_length,
            self.shoulders_distance,
            self.shoulders_width,
            self.upper_arm_length,
            self.lower_arm_length,
            self.hand_y,
            self.hand_z,
        ];
        if lengths
            .iter()
            .any(|x| !x.is_finite() || *x < 0.0 || *x > 3.0)
        {
            return Err("bone lengths must be finite meters in 0..3".into());
        }
        let offsets = [
            self.head_shift,
            self.foot_shift,
            self.chest_offset,
            self.hip_offset,
            self.skeleton_offset,
            self.elbow_offset,
        ];
        if offsets.iter().any(|x| !x.is_finite() || x.abs() > 3.0) {
            return Err("bone offsets must be finite meters in -3..3".into());
        }
        let ratios = [
            self.waist_from_chest_hip,
            self.waist_from_chest_legs,
            self.hip_from_chest_legs,
            self.hip_from_waist_legs,
            self.hip_legs,
            self.knee_tracker_ankle,
            self.knee_ankle,
        ];
        if ratios
            .iter()
            .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
        {
            return Err("skeleton averaging ratios must be in 0..1".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadPose {
    pub rotation: Q,
    pub position: Option<V>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BonePose {
    pub head: V,
    pub tail: V,
    pub rotation: Q,
    pub rotation_offset: Q,
    pub length: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ComputedPose {
    pub position: V,
    pub rotation: Q,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BoneLinks {
    pub parent: Option<String>,
    pub children: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SkeletonPose {
    pub bones: BTreeMap<String, BonePose>,
    pub hierarchy: BTreeMap<String, BoneLinks>,
    pub computed: BTreeMap<String, ComputedPose>,
    pub world_anchor_present: bool,
}
impl SkeletonPose {
    fn link(&mut self, name: &str, parent: Option<&str>) {
        self.hierarchy.insert(
            name.into(),
            BoneLinks {
                parent: parent.map(str::to_owned),
                children: Vec::new(),
            },
        );
        if let Some(parent) = parent {
            self.hierarchy
                .get_mut(parent)
                .expect("parent solved before child")
                .children
                .push(name.into());
        }
    }
}

pub struct Skeleton {
    pub config: SkeletonConfig,
    directions: BTreeMap<String, Q>,
    raw_rotations: BTreeMap<String, Q>,
    root_override: Option<V>,
}
impl Skeleton {
    pub fn new(config: SkeletonConfig) -> Self {
        Self {
            config,
            directions: BTreeMap::new(),
            raw_rotations: BTreeMap::new(),
            root_override: None,
        }
    }
    pub fn set_root_override(&mut self, root: Option<V>) {
        self.root_override = root;
    }
    pub fn previous_hand_rotation(&self, left: bool) -> Q {
        self.raw_rotations
            .get(if left {
                "left_hand_tracker"
            } else {
                "right_hand_tracker"
            })
            .copied()
            .unwrap_or(Q::IDENTITY)
    }
    pub fn solve(
        &mut self,
        inputs: &BTreeMap<BodyPosition, Q>,
        head: Option<HeadPose>,
        paused: bool,
    ) -> Result<SkeletonPose, String> {
        self.solve_with_positions(inputs, &BTreeMap::new(), head, paused)
    }
    pub fn solve_with_positions(
        &mut self,
        inputs: &BTreeMap<BodyPosition, Q>,
        positions: &BTreeMap<BodyPosition, V>,
        head: Option<HeadPose>,
        paused: bool,
    ) -> Result<SkeletonPose, String> {
        use BodyPosition::*;
        let c = self.config;
        let first =
            |positions: &[BodyPosition]| positions.iter().find_map(|p| inputs.get(p).copied());
        let head_rot = head
            .map(|h| h.rotation)
            .or_else(|| first(&[Head, Neck, UpperChest, Chest, Waist, Hip]))
            .unwrap_or(Q::IDENTITY);
        let neck = inputs.get(&Neck).copied().unwrap_or(head_rot);
        let mut dirs = self.directions.clone();
        let mut assigned_bones: BTreeSet<String> = ["head", "head_tracker", "neck"]
            .into_iter()
            .map(String::from)
            .collect();
        for name in ["head", "head_tracker"] {
            dirs.insert(name.into(), head_rot);
        }
        dirs.insert("neck".into(), neck);
        if !paused {
            let spine = first(&[UpperChest, Chest, Waist, Hip]).is_some();
            if spine || head.is_some() || inputs.contains_key(&Head) {
                for name in [
                    "upper_chest",
                    "chest_tracker",
                    "chest",
                    "waist",
                    "hip",
                    "hip_tracker",
                ] {
                    assigned_bones.insert(name.into());
                }
            }
            // Kotlin queries neck.worldTransform before updateBones: this is the previous frame.
            let fallback = |name: &str| {
                if head.is_some() || inputs.contains_key(&Head) {
                    self.directions
                        .get("neck")
                        .copied()
                        .unwrap_or(Q::IDENTITY)
                        .yaw_projection()
                } else {
                    self.directions.get(name).copied().unwrap_or(Q::IDENTITY)
                }
            };
            let upper =
                first(&[UpperChest, Chest, Waist, Hip]).unwrap_or_else(|| fallback("upper_chest"));
            let chest =
                first(&[Chest, UpperChest, Waist, Hip]).unwrap_or_else(|| fallback("chest"));
            let mut waist =
                first(&[Waist, Chest, Hip, UpperChest]).unwrap_or_else(|| fallback("waist"));
            let mut hip =
                first(&[Hip, Waist, Chest, UpperChest]).unwrap_or_else(|| fallback("hip"));
            let knees = inputs.contains_key(&LeftUpperLeg) && inputs.contains_key(&RightUpperLeg);
            let legs = inputs
                .get(&LeftUpperLeg)
                .copied()
                .unwrap_or(Q::IDENTITY)
                .lerp_q(
                    inputs.get(&RightUpperLeg).copied().unwrap_or(Q::IDENTITY),
                    0.5,
                );
            if c.extended_spine && spine {
                if !inputs.contains_key(&Waist) {
                    if let Some(chest) = first(&[Chest, UpperChest]) {
                        if let Some(hip) = inputs.get(&Hip) {
                            waist = chest.interp_q(*hip, c.waist_from_chest_hip);
                        } else if knees {
                            waist = chest.interp_q(legs, c.waist_from_chest_legs).unit();
                        }
                    }
                }
                if !inputs.contains_key(&Hip) && knees {
                    if let Some(waist) = inputs.get(&Waist) {
                        hip = waist.interp_q(legs, c.hip_from_waist_legs).unit();
                    } else if let Some(chest) = first(&[Chest, UpperChest]) {
                        hip = chest.interp_q(legs, c.hip_from_chest_legs).unit();
                    }
                }
            }
            if c.extended_pelvis && knees && !inputs.contains_key(&Hip) {
                assigned_bones.insert("hip".into());
                assigned_bones.insert("hip_tracker".into());
                let r = hip.inv() * (inputs[&LeftUpperLeg] + inputs[&RightUpperLeg]);
                let extended = (hip * r * Q::new(r.w, -r.x, 0.0, 0.0)).unit();
                hip = hip.interp_r(
                    if extended.len_sq() != 0.0 {
                        extended
                    } else {
                        Q::IDENTITY
                    },
                    c.hip_legs,
                );
            }
            for (name, q) in [
                ("upper_chest", upper),
                ("chest_tracker", upper),
                ("chest", chest),
                ("waist", waist),
                ("hip", hip),
                ("hip_tracker", hip),
                ("left_hip", hip),
                ("right_hip", hip),
            ] {
                dirs.insert(name.into(), q);
            }
            assigned_bones.insert("left_hip".into());
            assigned_bones.insert("right_hip".into());
            for (side, upper_part, lower_part, foot_part) in [
                ("left", LeftUpperLeg, LeftLowerLeg, LeftFoot),
                ("right", RightUpperLeg, RightLowerLeg, RightFoot),
            ] {
                let mut upper = inputs
                    .get(&upper_part)
                    .copied()
                    .unwrap_or_else(|| hip.yaw_projection());
                let lower = inputs
                    .get(&lower_part)
                    .copied()
                    .unwrap_or_else(|| upper.yaw_projection());
                let foot = inputs.get(&foot_part).copied().unwrap_or(lower);
                let mut knee = upper;
                if c.extended_knee {
                    if let (Some(u), Some(l)) = (inputs.get(&upper_part), inputs.get(&lower_part)) {
                        let r = u.inv() * *l;
                        let extended = (*u * r * Q::new(r.w, -r.x, 0.0, 0.0)).unit();
                        upper = u.interp_r(extended, c.knee_ankle);
                        knee = u.interp_r(extended, c.knee_tracker_ankle);
                    }
                }
                for (name, q) in [
                    ("upper_leg", upper),
                    ("knee_tracker", knee),
                    ("lower_leg", lower),
                    ("foot", foot),
                    ("foot_tracker", foot),
                ] {
                    let name = format!("{side}_{name}");
                    assigned_bones.insert(name.clone());
                    dirs.insert(name, q);
                }
            }
        }
        let mut out = SkeletonPose {
            world_anchor_present: head.is_some_and(|h| h.position.is_some()),
            ..Default::default()
        };
        let root = head
            .and_then(|h| h.position)
            .or(self.root_override)
            .unwrap_or(V::ZERO);
        let anchored = out.world_anchor_present;
        let entries = vec![
            (
                "head",
                None,
                V {
                    x: 0.0,
                    y: 0.0,
                    z: if anchored { c.head_shift } else { 0.0 },
                },
            ),
            (
                "neck",
                Some("head"),
                V {
                    x: 0.0,
                    y: if anchored { -c.neck_length } else { 0.0 },
                    z: 0.0,
                },
            ),
            (
                "upper_chest",
                Some("neck"),
                V {
                    x: 0.0,
                    y: -c.upper_chest_length,
                    z: 0.0,
                },
            ),
            (
                "chest",
                Some("upper_chest"),
                V {
                    x: 0.0,
                    y: -c.chest_length,
                    z: 0.0,
                },
            ),
            (
                "waist",
                Some("chest"),
                V {
                    x: 0.0,
                    y: -c.waist_length,
                    z: 0.0,
                },
            ),
            (
                "hip",
                Some("waist"),
                V {
                    x: 0.0,
                    y: -c.hip_length,
                    z: 0.0,
                },
            ),
            (
                "left_hip",
                Some("hip"),
                V {
                    x: -c.hips_width / 2.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                "right_hip",
                Some("hip"),
                V {
                    x: c.hips_width / 2.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                "left_upper_leg",
                Some("left_hip"),
                V {
                    x: 0.0,
                    y: -c.upper_leg_length,
                    z: 0.0,
                },
            ),
            (
                "left_lower_leg",
                Some("left_upper_leg"),
                V {
                    x: 0.0,
                    y: -c.lower_leg_length,
                    z: -c.foot_shift,
                },
            ),
            (
                "left_foot",
                Some("left_lower_leg"),
                V {
                    x: 0.0,
                    y: 0.0,
                    z: -c.foot_length,
                },
            ),
            (
                "right_upper_leg",
                Some("right_hip"),
                V {
                    x: 0.0,
                    y: -c.upper_leg_length,
                    z: 0.0,
                },
            ),
            (
                "right_lower_leg",
                Some("right_upper_leg"),
                V {
                    x: 0.0,
                    y: -c.lower_leg_length,
                    z: -c.foot_shift,
                },
            ),
            (
                "right_foot",
                Some("right_lower_leg"),
                V {
                    x: 0.0,
                    y: 0.0,
                    z: -c.foot_length,
                },
            ),
            ("head_tracker", Some("neck"), V::ZERO),
            (
                "chest_tracker",
                Some("upper_chest"),
                V {
                    x: 0.0,
                    y: -c.chest_length - c.chest_offset,
                    z: -c.skeleton_offset,
                },
            ),
            (
                "hip_tracker",
                Some("hip"),
                V {
                    x: 0.0,
                    y: -c.hip_offset,
                    z: -c.skeleton_offset,
                },
            ),
            (
                "left_knee_tracker",
                Some("left_upper_leg"),
                V {
                    x: 0.0,
                    y: 0.0,
                    z: -c.skeleton_offset,
                },
            ),
            (
                "right_knee_tracker",
                Some("right_upper_leg"),
                V {
                    x: 0.0,
                    y: 0.0,
                    z: -c.skeleton_offset,
                },
            ),
            (
                "left_foot_tracker",
                Some("left_foot"),
                V {
                    x: 0.0,
                    y: 0.0,
                    z: -c.skeleton_offset,
                },
            ),
            (
                "right_foot_tracker",
                Some("right_foot"),
                V {
                    x: 0.0,
                    y: 0.0,
                    z: -c.skeleton_offset,
                },
            ),
        ];
        for (name, parent, offset) in entries {
            let assigned = dirs.get(name).copied().unwrap_or(Q::IDENTITY);
            let length = offset.len();
            let rotation_offset = if length == 0.0 {
                Q::IDENTITY
            } else if offset.unit().y == 1.0 {
                Q::new(0.0, 1.0, 0.0, 0.0)
            } else {
                Q::from_to(V::DOWN, offset)
            };
            // Bones keep raw identity until setRotation() assigns their direction.
            // Paused or unassigned spine bones retain that stored rotation.
            let mut rotation = if assigned_bones.contains(name) {
                assigned * rotation_offset
            } else {
                self.raw_rotations.get(name).copied().unwrap_or(Q::IDENTITY)
            };
            if c.enforce_constraints {
                if let Some(parent_name) = parent {
                    let p = out.bones[parent_name];
                    rotation = crate::constraints::Constraint::for_bone(name).world(
                        rotation,
                        p.rotation,
                        p.rotation_offset,
                        rotation_offset,
                    );
                }
            }
            if !rotation.is_rotation() {
                return Err(format!("invalid derived rotation for {name}"));
            }
            let head = parent.map(|p| out.bones[p].tail).unwrap_or(root);
            let tail = head
                + rotation.rotate(V {
                    x: 0.0,
                    y: -length,
                    z: 0.0,
                });
            if !tail.is_finite() {
                return Err(format!("invalid derived position for {name}"));
            }
            out.link(name, parent);
            out.bones.insert(
                name.into(),
                BonePose {
                    head,
                    tail,
                    rotation,
                    rotation_offset,
                    length,
                },
            );
            if name.ends_with("_tracker") {
                out.computed.insert(
                    name.trim_end_matches("_tracker").into(),
                    ComputedPose {
                        position: tail,
                        rotation: rotation * rotation_offset.inv(),
                    },
                );
            }
        }
        self.solve_arms(inputs, positions, &mut dirs, &mut out, paused)?;
        self.directions = dirs;
        self.raw_rotations = out
            .bones
            .iter()
            .map(|(name, bone)| (name.clone(), bone.rotation))
            .collect();
        Ok(out)
    }

    fn solve_arms(
        &self,
        inputs: &BTreeMap<BodyPosition, Q>,
        positions: &BTreeMap<BodyPosition, V>,
        dirs: &mut BTreeMap<String, Q>,
        out: &mut SkeletonPose,
        paused: bool,
    ) -> Result<(), String> {
        use BodyPosition::*;
        let c = self.config;
        for (side, shoulder, upper, lower, hand, fingers) in [
            (
                "left",
                LeftShoulder,
                LeftUpperArm,
                LeftLowerArm,
                LeftHand,
                [
                    [LeftThumbMetacarpal, LeftThumbProximal, LeftThumbDistal],
                    [LeftIndexProximal, LeftIndexIntermediate, LeftIndexDistal],
                    [LeftMiddleProximal, LeftMiddleIntermediate, LeftMiddleDistal],
                    [LeftRingProximal, LeftRingIntermediate, LeftRingDistal],
                    [LeftLittleProximal, LeftLittleIntermediate, LeftLittleDistal],
                ],
            ),
            (
                "right",
                RightShoulder,
                RightUpperArm,
                RightLowerArm,
                RightHand,
                [
                    [RightThumbMetacarpal, RightThumbProximal, RightThumbDistal],
                    [RightIndexProximal, RightIndexIntermediate, RightIndexDistal],
                    [
                        RightMiddleProximal,
                        RightMiddleIntermediate,
                        RightMiddleDistal,
                    ],
                    [RightRingProximal, RightRingIntermediate, RightRingDistal],
                    [
                        RightLittleProximal,
                        RightLittleIntermediate,
                        RightLittleDistal,
                    ],
                ],
            ),
        ] {
            let controller = !c.force_arms_from_hmd && positions.contains_key(&hand);
            let chest = dirs.get("upper_chest").copied().unwrap_or(Q::IDENTITY);
            let shoulder_q = inputs.get(&shoulder).copied().unwrap_or(chest);
            let upper_q = inputs
                .get(&upper)
                .or_else(|| inputs.get(&lower))
                .copied()
                .unwrap_or(if controller { Q::IDENTITY } else { chest });
            let lower_q = inputs
                .get(&lower)
                .or_else(|| inputs.get(&upper))
                .copied()
                .unwrap_or(if controller { Q::IDENTITY } else { chest });
            let hand_q = inputs.get(&hand).copied().unwrap_or(lower_q);
            let name = |suffix: &str| format!("{side}_{suffix}");
            let upper_shoulder = name("upper_shoulder");
            let shoulder_name = name("shoulder");
            let upper_name = name("upper_arm");
            let lower_name = name("lower_arm");
            let hand_name = name("hand");
            let elbow_name = name("elbow_tracker");
            let hand_tracker = name("hand_tracker");
            let sign = if side == "left" { -1.0 } else { 1.0 };
            let mut entries = vec![
                (
                    upper_shoulder.clone(),
                    Some("neck".to_string()),
                    V::ZERO,
                    chest,
                    None,
                ),
                (
                    shoulder_name.clone(),
                    Some(upper_shoulder.clone()),
                    V::new(sign * c.shoulders_width / 2.0, -c.shoulders_distance, 0.0),
                    shoulder_q,
                    None,
                ),
                (
                    upper_name.clone(),
                    Some(shoulder_name.clone()),
                    V::new(0.0, -c.upper_arm_length, 0.0),
                    upper_q,
                    None,
                ),
            ];
            if controller {
                entries.extend([
                    (
                        hand_tracker.clone(),
                        None,
                        V::ZERO,
                        hand_q,
                        positions.get(&hand).copied(),
                    ),
                    (
                        hand_name.clone(),
                        Some(hand_tracker.clone()),
                        V::new(0.0, c.hand_y, c.hand_z),
                        hand_q,
                        None,
                    ),
                    (
                        lower_name.clone(),
                        Some(hand_name.clone()),
                        V::new(0.0, c.lower_arm_length, 0.0),
                        lower_q,
                        None,
                    ),
                    (
                        elbow_name.clone(),
                        Some(lower_name.clone()),
                        V::new(0.0, -c.elbow_offset, 0.0),
                        upper_q,
                        None,
                    ),
                ]);
            } else {
                entries.extend([
                    (
                        lower_name.clone(),
                        Some(upper_name.clone()),
                        V::new(0.0, -c.lower_arm_length, 0.0),
                        lower_q,
                        None,
                    ),
                    (
                        hand_name.clone(),
                        Some(lower_name.clone()),
                        V::new(0.0, -c.hand_y, -c.hand_z),
                        hand_q,
                        None,
                    ),
                    (
                        elbow_name.clone(),
                        Some(upper_name.clone()),
                        V::new(0.0, -c.elbow_offset, 0.0),
                        upper_q,
                        None,
                    ),
                    (
                        hand_tracker.clone(),
                        Some(hand_name.clone()),
                        V::ZERO,
                        hand_q,
                        None,
                    ),
                ]);
            }
            for (name, parent, offset, rotation, root) in entries {
                let retain = paused
                    || (controller
                        && (name == upper_name || name == shoulder_name || name == upper_shoulder));
                let q = if retain {
                    let raw = self
                        .raw_rotations
                        .get(&name)
                        .copied()
                        .unwrap_or(Q::IDENTITY);
                    let off = if offset.len() == 0.0 {
                        Q::IDENTITY
                    } else if offset.unit().y == 1.0 {
                        Q::new(0.0, 1.0, 0.0, 0.0)
                    } else {
                        Q::from_to(V::DOWN, offset)
                    };
                    raw * off.inv()
                } else {
                    rotation
                };
                Self::append_bone(
                    out,
                    &name,
                    parent.as_deref(),
                    offset,
                    q,
                    root,
                    c.enforce_constraints
                        && !(controller
                            && (name == hand_tracker
                                || name == hand_name
                                || name == lower_name
                                || name == elbow_name)),
                )?;
                dirs.insert(name, q);
            }
            let hand_q = self
                .raw_rotations
                .get(&hand_tracker)
                .copied()
                .unwrap_or(Q::IDENTITY);
            for (index, parts) in fingers.into_iter().enumerate() {
                let finger = ["thumb", "index", "middle", "ring", "little"][index];
                let segments = if index == 0 {
                    ["metacarpal", "proximal", "distal"]
                } else {
                    ["proximal", "intermediate", "distal"]
                };
                let base_length = c.hand_y * [0.2, 0.25, 0.3, 0.28, 0.2][index];
                // SkeletonConfigManager phalanxLengthDivider.
                let ratios = [2.3, 1.3, 1.0];
                let mut qs = [hand_q; 3];
                if let Some(p) = inputs.get(&parts[0]) {
                    qs[0] = *p;
                    if !inputs.contains_key(&parts[1]) {
                        qs[1] = hand_q.interp_q(*p, 2.12);
                    }
                    if !inputs.contains_key(&parts[2]) {
                        qs[2] = hand_q.interp_q(*p, 3.03);
                    }
                }
                if let Some(p) = inputs.get(&parts[1]) {
                    if !inputs.contains_key(&parts[0]) {
                        qs[0] = hand_q.interp_q(*p, 0.47);
                    }
                    qs[1] = *p;
                    if !inputs.contains_key(&parts[2]) {
                        qs[2] = hand_q.interp_q(*p, 1.43);
                    }
                }
                if let Some(p) = inputs.get(&parts[2]) {
                    if !inputs.contains_key(&parts[0]) && !inputs.contains_key(&parts[1]) {
                        qs[0] = hand_q.interp_q(*p, 0.33);
                    }
                    if !inputs.contains_key(&parts[1]) {
                        qs[1] = hand_q.interp_q(*p, 0.7);
                    }
                    qs[2] = *p;
                }
                let mut parent = hand_name.clone();
                for j in 0..3 {
                    let n = format!("{side}_{finger}_{}", segments[j]);
                    let q = if paused {
                        dirs.get(&n).copied().unwrap_or(Q::IDENTITY)
                    } else {
                        qs[j]
                    };
                    Self::append_bone(
                        out,
                        &n,
                        Some(&parent),
                        V::new(
                            0.0,
                            -base_length * 3.0 * ratios[j] / 4.6,
                            if index == 0 {
                                -base_length * 3.0 * ratios[j] / 4.6 * 0.5
                            } else {
                                0.0
                            },
                        ),
                        q,
                        None,
                        c.enforce_constraints && !controller,
                    )?;
                    dirs.insert(n.clone(), q);
                    parent = n;
                }
            }
        }
        Ok(())
    }
    fn append_bone(
        out: &mut SkeletonPose,
        name: &str,
        parent: Option<&str>,
        offset: V,
        q: Q,
        root: Option<V>,
        constraints: bool,
    ) -> Result<(), String> {
        let length = offset.len();
        let rotation_offset = if length == 0.0 {
            Q::IDENTITY
        } else if offset.unit().y == 1.0 {
            Q::new(0.0, 1.0, 0.0, 0.0)
        } else {
            Q::from_to(V::DOWN, offset)
        };
        let mut rotation = q * rotation_offset;
        if constraints {
            if let Some(p) = parent {
                let p = out.bones[p];
                rotation = crate::constraints::Constraint::for_bone(name).world(
                    rotation,
                    p.rotation,
                    p.rotation_offset,
                    rotation_offset,
                );
            }
        }
        let head = parent
            .map(|p| out.bones[p].tail)
            .or(root)
            .unwrap_or(V::ZERO);
        let tail = head + rotation.rotate(V::new(0.0, -length, 0.0));
        if !rotation.is_rotation() || !tail.is_finite() {
            return Err(format!("invalid arm/finger bone {name}"));
        }
        out.link(name, parent);
        out.bones.insert(
            name.into(),
            BonePose {
                head,
                tail,
                rotation,
                rotation_offset,
                length,
            },
        );
        if name.ends_with("_tracker") {
            out.computed.insert(
                name.trim_end_matches("_tracker").into(),
                ComputedPose {
                    position: tail,
                    rotation: rotation * rotation_offset.inv(),
                },
            );
        }
        Ok(())
    }
}
