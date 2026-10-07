//! UnityArmature topology and UnityBone mapping ported from the pinned Kotlin sources.
use slimevr_core::{skeleton::BodyPosition as B, Quaternion as Q, Vector3 as V};
#[derive(Clone, Copy)]
pub struct Bone {
    pub name: &'static str,
    pub vrm_name: &'static str,
    pub node: usize,
    pub body: B,
    pub slime_bone: &'static str,
}
pub const BONES: &[Bone] = &[
    Bone {
        name: "Hips",
        vrm_name: "hips",
        node: 7,
        body: B::Hip,
        slime_bone: "hip",
    },
    Bone {
        name: "LeftUpperLeg",
        vrm_name: "leftUpperLeg",
        node: 8,
        body: B::LeftUpperLeg,
        slime_bone: "left_upper_leg",
    },
    Bone {
        name: "RightUpperLeg",
        vrm_name: "rightUpperLeg",
        node: 9,
        body: B::RightUpperLeg,
        slime_bone: "right_upper_leg",
    },
    Bone {
        name: "LeftLowerLeg",
        vrm_name: "leftLowerLeg",
        node: 10,
        body: B::LeftLowerLeg,
        slime_bone: "left_lower_leg",
    },
    Bone {
        name: "RightLowerLeg",
        vrm_name: "rightLowerLeg",
        node: 13,
        body: B::RightLowerLeg,
        slime_bone: "right_lower_leg",
    },
    Bone {
        name: "LeftFoot",
        vrm_name: "leftFoot",
        node: 11,
        body: B::LeftFoot,
        slime_bone: "left_foot",
    },
    Bone {
        name: "RightFoot",
        vrm_name: "rightFoot",
        node: 14,
        body: B::RightFoot,
        slime_bone: "right_foot",
    },
    Bone {
        name: "Spine",
        vrm_name: "spine",
        node: 6,
        body: B::Waist,
        slime_bone: "waist",
    },
    Bone {
        name: "Chest",
        vrm_name: "chest",
        node: 5,
        body: B::Chest,
        slime_bone: "chest",
    },
    Bone {
        name: "UpperChest",
        vrm_name: "upperChest",
        node: 4,
        body: B::Chest,
        slime_bone: "chest",
    },
    Bone {
        name: "Neck",
        vrm_name: "neck",
        node: 2,
        body: B::Neck,
        slime_bone: "neck",
    },
    Bone {
        name: "Head",
        vrm_name: "head",
        node: 1,
        body: B::Head,
        slime_bone: "head",
    },
    Bone {
        name: "LeftShoulder",
        vrm_name: "leftShoulder",
        node: 16,
        body: B::LeftShoulder,
        slime_bone: "left_shoulder",
    },
    Bone {
        name: "RightShoulder",
        vrm_name: "rightShoulder",
        node: 17,
        body: B::RightShoulder,
        slime_bone: "right_shoulder",
    },
    Bone {
        name: "LeftUpperArm",
        vrm_name: "leftUpperArm",
        node: 18,
        body: B::LeftUpperArm,
        slime_bone: "left_upper_arm",
    },
    Bone {
        name: "RightUpperArm",
        vrm_name: "rightUpperArm",
        node: 19,
        body: B::RightUpperArm,
        slime_bone: "right_upper_arm",
    },
    Bone {
        name: "LeftLowerArm",
        vrm_name: "leftLowerArm",
        node: 20,
        body: B::LeftLowerArm,
        slime_bone: "left_lower_arm",
    },
    Bone {
        name: "RightLowerArm",
        vrm_name: "rightLowerArm",
        node: 21,
        body: B::RightLowerArm,
        slime_bone: "right_lower_arm",
    },
    Bone {
        name: "LeftHand",
        vrm_name: "leftHand",
        node: 22,
        body: B::LeftHand,
        slime_bone: "left_hand",
    },
    Bone {
        name: "RightHand",
        vrm_name: "rightHand",
        node: 23,
        body: B::RightHand,
        slime_bone: "right_hand",
    },
    Bone {
        name: "LeftThumbProximal",
        vrm_name: "leftThumbMetacarpal",
        node: 26,
        body: B::LeftThumbMetacarpal,
        slime_bone: "left_thumb_metacarpal",
    },
    Bone {
        name: "LeftThumbIntermediate",
        vrm_name: "leftThumbProximal",
        node: 27,
        body: B::LeftThumbProximal,
        slime_bone: "left_thumb_proximal",
    },
    Bone {
        name: "LeftThumbDistal",
        vrm_name: "leftThumbDistal",
        node: 28,
        body: B::LeftThumbDistal,
        slime_bone: "left_thumb_distal",
    },
    Bone {
        name: "LeftIndexProximal",
        vrm_name: "leftIndexProximal",
        node: 30,
        body: B::LeftIndexProximal,
        slime_bone: "left_index_proximal",
    },
    Bone {
        name: "LeftIndexIntermediate",
        vrm_name: "leftIndexIntermediate",
        node: 31,
        body: B::LeftIndexIntermediate,
        slime_bone: "left_index_intermediate",
    },
    Bone {
        name: "LeftIndexDistal",
        vrm_name: "leftIndexDistal",
        node: 32,
        body: B::LeftIndexDistal,
        slime_bone: "left_index_distal",
    },
    Bone {
        name: "LeftMiddleProximal",
        vrm_name: "leftMiddleProximal",
        node: 34,
        body: B::LeftMiddleProximal,
        slime_bone: "left_middle_proximal",
    },
    Bone {
        name: "LeftMiddleIntermediate",
        vrm_name: "leftMiddleIntermediate",
        node: 35,
        body: B::LeftMiddleIntermediate,
        slime_bone: "left_middle_intermediate",
    },
    Bone {
        name: "LeftMiddleDistal",
        vrm_name: "leftMiddleDistal",
        node: 36,
        body: B::LeftMiddleDistal,
        slime_bone: "left_middle_distal",
    },
    Bone {
        name: "LeftRingProximal",
        vrm_name: "leftRingProximal",
        node: 38,
        body: B::LeftRingProximal,
        slime_bone: "left_ring_proximal",
    },
    Bone {
        name: "LeftRingIntermediate",
        vrm_name: "leftRingIntermediate",
        node: 39,
        body: B::LeftRingIntermediate,
        slime_bone: "left_ring_intermediate",
    },
    Bone {
        name: "LeftRingDistal",
        vrm_name: "leftRingDistal",
        node: 40,
        body: B::LeftRingDistal,
        slime_bone: "left_ring_distal",
    },
    Bone {
        name: "LeftLittleProximal",
        vrm_name: "leftLittleProximal",
        node: 42,
        body: B::LeftLittleProximal,
        slime_bone: "left_little_proximal",
    },
    Bone {
        name: "LeftLittleIntermediate",
        vrm_name: "leftLittleIntermediate",
        node: 43,
        body: B::LeftLittleIntermediate,
        slime_bone: "left_little_intermediate",
    },
    Bone {
        name: "LeftLittleDistal",
        vrm_name: "leftLittleDistal",
        node: 44,
        body: B::LeftLittleDistal,
        slime_bone: "left_little_distal",
    },
    Bone {
        name: "RightThumbProximal",
        vrm_name: "rightThumbMetacarpal",
        node: 46,
        body: B::RightThumbMetacarpal,
        slime_bone: "right_thumb_metacarpal",
    },
    Bone {
        name: "RightThumbIntermediate",
        vrm_name: "rightThumbProximal",
        node: 47,
        body: B::RightThumbProximal,
        slime_bone: "right_thumb_proximal",
    },
    Bone {
        name: "RightThumbDistal",
        vrm_name: "rightThumbDistal",
        node: 48,
        body: B::RightThumbDistal,
        slime_bone: "right_thumb_distal",
    },
    Bone {
        name: "RightIndexProximal",
        vrm_name: "rightIndexProximal",
        node: 50,
        body: B::RightIndexProximal,
        slime_bone: "right_index_proximal",
    },
    Bone {
        name: "RightIndexIntermediate",
        vrm_name: "rightIndexIntermediate",
        node: 51,
        body: B::RightIndexIntermediate,
        slime_bone: "right_index_intermediate",
    },
    Bone {
        name: "RightIndexDistal",
        vrm_name: "rightIndexDistal",
        node: 52,
        body: B::RightIndexDistal,
        slime_bone: "right_index_distal",
    },
    Bone {
        name: "RightMiddleProximal",
        vrm_name: "rightMiddleProximal",
        node: 54,
        body: B::RightMiddleProximal,
        slime_bone: "right_middle_proximal",
    },
    Bone {
        name: "RightMiddleIntermediate",
        vrm_name: "rightMiddleIntermediate",
        node: 55,
        body: B::RightMiddleIntermediate,
        slime_bone: "right_middle_intermediate",
    },
    Bone {
        name: "RightMiddleDistal",
        vrm_name: "rightMiddleDistal",
        node: 56,
        body: B::RightMiddleDistal,
        slime_bone: "right_middle_distal",
    },
    Bone {
        name: "RightRingProximal",
        vrm_name: "rightRingProximal",
        node: 58,
        body: B::RightRingProximal,
        slime_bone: "right_ring_proximal",
    },
    Bone {
        name: "RightRingIntermediate",
        vrm_name: "rightRingIntermediate",
        node: 59,
        body: B::RightRingIntermediate,
        slime_bone: "right_ring_intermediate",
    },
    Bone {
        name: "RightRingDistal",
        vrm_name: "rightRingDistal",
        node: 60,
        body: B::RightRingDistal,
        slime_bone: "right_ring_distal",
    },
    Bone {
        name: "RightLittleProximal",
        vrm_name: "rightLittleProximal",
        node: 62,
        body: B::RightLittleProximal,
        slime_bone: "right_little_proximal",
    },
    Bone {
        name: "RightLittleIntermediate",
        vrm_name: "rightLittleIntermediate",
        node: 63,
        body: B::RightLittleIntermediate,
        slime_bone: "right_little_intermediate",
    },
    Bone {
        name: "RightLittleDistal",
        vrm_name: "rightLittleDistal",
        node: 64,
        body: B::RightLittleDistal,
        slime_bone: "right_little_distal",
    },
];
const TOPOLOGY: &[(Option<usize>, bool)] = &[
    (Some(1), false),  // headNode
    (Some(2), false),  // neckTailNode
    (Some(3), false),  // neckHeadNode
    (Some(4), false),  // upperChestNode
    (Some(5), false),  // chestNode
    (Some(6), false),  // spineTailNode
    (Some(7), false),  // spineHeadNode
    (None, false),     // hipsNode
    (Some(7), false),  // leftHipNode
    (Some(7), false),  // rightHipNode
    (Some(8), false),  // leftKneeNode
    (Some(10), false), // leftAnkleNode
    (Some(11), false), // leftFootNode
    (Some(9), false),  // rightKneeNode
    (Some(13), false), // rightAnkleNode
    (Some(14), false), // rightFootNode
    (Some(3), false),  // leftShoulderHeadNode
    (Some(3), false),  // rightShoulderHeadNode
    (Some(16), false), // leftShoulderTailNode
    (Some(17), false), // rightShoulderTailNode
    (Some(18), false), // leftElbowNode
    (Some(19), false), // rightElbowNode
    (Some(20), false), // leftWristNode
    (Some(21), false), // rightWristNode
    (Some(22), true),  // leftHandNode
    (Some(23), true),  // rightHandNode
    (Some(24), false), // leftThumbProximalHeadNode
    (Some(26), false), // leftThumbProximalTailNode
    (Some(27), false), // leftThumbIntermediateNode
    (Some(28), false), // leftThumbDistalNode
    (Some(24), false), // leftIndexProximalHeadNode
    (Some(30), false), // leftIndexProximalTailNode
    (Some(31), false), // leftIndexIntermediateNode
    (Some(32), false), // leftIndexDistalNode
    (Some(24), false), // leftMiddleProximalHeadNode
    (Some(34), false), // leftMiddleProximalTailNode
    (Some(35), false), // leftMiddleIntermediateNode
    (Some(36), false), // leftMiddleDistalNode
    (Some(24), false), // leftRingProximalHeadNode
    (Some(38), false), // leftRingProximalTailNode
    (Some(39), false), // leftRingIntermediateNode
    (Some(40), false), // leftRingDistalNode
    (Some(24), false), // leftLittleProximalHeadNode
    (Some(42), false), // leftLittleProximalTailNode
    (Some(43), false), // leftLittleIntermediateNode
    (Some(44), false), // leftLittleDistalNode
    (Some(25), false), // rightThumbProximalHeadNode
    (Some(46), false), // rightThumbProximalTailNode
    (Some(47), false), // rightThumbIntermediateNode
    (Some(48), false), // rightThumbDistalNode
    (Some(25), false), // rightIndexProximalHeadNode
    (Some(50), false), // rightIndexProximalTailNode
    (Some(51), false), // rightIndexIntermediateNode
    (Some(52), false), // rightIndexDistalNode
    (Some(25), false), // rightMiddleProximalHeadNode
    (Some(54), false), // rightMiddleProximalTailNode
    (Some(55), false), // rightMiddleIntermediateNode
    (Some(56), false), // rightMiddleDistalNode
    (Some(25), false), // rightRingProximalHeadNode
    (Some(58), false), // rightRingProximalTailNode
    (Some(59), false), // rightRingIntermediateNode
    (Some(60), false), // rightRingDistalNode
    (Some(25), false), // rightLittleProximalHeadNode
    (Some(62), false), // rightLittleProximalTailNode
    (Some(63), false), // rightLittleIntermediateNode
    (Some(64), false), // rightLittleDistalNode
];
const ROOT: usize = 7;
const CHEST: usize = 4;
const UPPER_CHEST: usize = 3;
const LEFT_HIP: usize = 8;
const RIGHT_HIP: usize = 9;
const HEAD_PARENT: usize = 2;
#[derive(Clone, Copy)]
struct Node {
    rotation: Q,
    translation: V,
    world_rotation: Q,
    world_translation: V,
}
impl Default for Node {
    fn default() -> Self {
        Self {
            rotation: Q::IDENTITY,
            translation: V::ZERO,
            world_rotation: Q::IDENTITY,
            world_translation: V::ZERO,
        }
    }
}
pub struct Armature {
    nodes: Vec<Node>,
    local: bool,
    root_position: V,
    root_rotation: Q,
}
impl Armature {
    pub fn new(local: bool) -> Self {
        Self {
            nodes: vec![Node::default(); TOPOLOGY.len()],
            local,
            root_position: V::ZERO,
            root_rotation: Q::IDENTITY,
        }
    }
    pub fn bone(name: &str) -> Option<&'static Bone> {
        BONES.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }
    pub fn root(&mut self, p: V, q: Q) {
        self.root_position = p;
        self.root_rotation = q;
    }
    pub fn set_local(&mut self, b: &Bone, q: Q) {
        if b.node == ROOT {
            self.nodes[ROOT].world_rotation = q;
        } else {
            self.nodes[b.node].rotation = q * start_arm_offset(b.body);
        }
    }
    pub fn set_global(&mut self, b: &Bone, q: Q) {
        self.nodes[b.node].rotation = q * arm_offset(b.body);
    }
    pub fn global(&self, b: &Bone) -> Q {
        self.nodes[b.node].world_rotation * self.root_rotation
    }
    pub fn local_rotation(&self, b: &Bone) -> Q {
        let node = self.nodes[b.node];
        if b.node == ROOT {
            node.world_rotation * self.root_rotation
        } else {
            self.nodes[TOPOLOGY[b.node].0.unwrap()].world_rotation.inv() * node.world_rotation
        }
    }
    pub fn local_translation(&self, b: &Bone) -> V {
        if b.node == ROOT {
            self.nodes[ROOT].world_translation * 2.0
                - (self.nodes[LEFT_HIP].world_translation + self.nodes[RIGHT_HIP].world_translation)
                    * 0.5
                + self.root_position
        } else {
            self.nodes[b.node].translation
        }
    }
    pub fn update(&mut self) {
        self.nodes[UPPER_CHEST].rotation = self.nodes[CHEST].rotation;
        self.update_node(ROOT);
    }
    fn update_node(&mut self, index: usize) {
        let mut n = self.nodes[index];
        n.world_rotation = n.rotation;
        n.world_translation = n.translation;
        if let Some(parent) = TOPOLOGY[index].0 {
            let p = self.nodes[parent];
            n.world_translation = p.world_rotation.rotate(n.translation) + p.world_translation;
            if self.local != TOPOLOGY[index].1 {
                n.world_rotation = p.world_rotation * n.rotation;
            }
        }
        self.nodes[index] = n;
        for (child, topology) in TOPOLOGY.iter().enumerate().take(self.nodes.len()) {
            if topology.0 == Some(index) {
                self.update_node(child);
            }
        }
    }
    pub fn load_vrm(&mut self, json: &str) -> Result<f32, String> {
        let offsets = vrm_offsets(json)?;
        for b in BONES {
            self.nodes[b.node].translation = offsets.get(b.name).copied().unwrap_or(V::ZERO);
        }
        let left = offsets.get("LeftUpperLeg").copied().unwrap_or(V::ZERO);
        let right = offsets.get("RightUpperLeg").copied().unwrap_or(V::ZERO);
        self.nodes[ROOT].translation = self.nodes[ROOT].translation + (left + right) * 0.5;
        Ok(["Hips", "Spine", "Chest", "UpperChest", "Neck", "Head"]
            .iter()
            .map(|name| offsets.get(*name).copied().unwrap_or(V::ZERO).y)
            .sum::<f32>()
            + left.y
            + right.y)
    }
    pub fn anchor_head(&mut self, p: V) {
        let head = self.nodes[HEAD_PARENT].world_translation;
        let hips = self.nodes[ROOT].world_translation;
        self.nodes[ROOT].translation = p - (head - hips);
    }
}
fn arm_offset(body: B) -> Q {
    let arm = if body.is_left_finger()
        || matches!(body, B::LeftUpperArm | B::LeftLowerArm | B::LeftHand)
    {
        1.0
    } else if body.is_right_finger()
        || matches!(body, B::RightUpperArm | B::RightLowerArm | B::RightHand)
    {
        -1.0
    } else {
        0.0
    };
    Q::rotation_z(arm * std::f32::consts::FRAC_PI_2)
}
fn start_arm_offset(body: B) -> Q {
    let start = matches!(
        body,
        B::LeftUpperArm
            | B::RightUpperArm
            | B::LeftThumbMetacarpal
            | B::RightThumbMetacarpal
            | B::LeftIndexProximal
            | B::RightIndexProximal
            | B::LeftMiddleProximal
            | B::RightMiddleProximal
            | B::LeftRingProximal
            | B::RightRingProximal
            | B::LeftLittleProximal
            | B::RightLittleProximal
    );
    if start {
        arm_offset(body).inv()
    } else {
        Q::IDENTITY
    }
}
pub fn vrm_offsets(json: &str) -> Result<std::collections::BTreeMap<String, V>, String> {
    let data: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid VRM JSON: {e}"))?;
    let nodes = data["nodes"].as_array().ok_or("VRM JSON lacks nodes")?;
    if nodes.len() > 65536 {
        return Err("too many VRM nodes".into());
    }
    let v1 = &data["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"];
    let v0 = &data["extensions"]["VRM"]["humanoid"]["humanBones"];
    if !v1.is_object() && !v0.is_array() {
        return Err("VRM JSON lacks humanoid humanBones".into());
    }
    let mut offsets = std::collections::BTreeMap::new();
    for b in BONES {
        let index = if v1.is_object() {
            v1[b.vrm_name]["node"].as_u64()
        } else {
            v0.as_array()
                .unwrap()
                .iter()
                .find(|v| {
                    v["bone"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(b.name))
                })
                .and_then(|v| v["node"].as_u64())
        };
        let Some(index) = index else {
            continue;
        };
        let node = nodes
            .get(index as usize)
            .ok_or("VRM bone node index out of bounds")?;
        let Some(t) = node["translation"].as_array() else {
            continue;
        };
        if t.len() != 3 {
            return Err("VRM translation needs three numbers".into());
        }
        let p = V::new(
            t[0].as_f64().ok_or("invalid VRM translation")? as f32,
            t[1].as_f64().ok_or("invalid VRM translation")? as f32,
            t[2].as_f64().ok_or("invalid VRM translation")? as f32,
        );
        if !p.is_finite() {
            return Err("invalid VRM translation".into());
        }
        offsets.insert(b.name.into(), p);
    }
    Ok(offsets)
}
pub fn opposite(body: B) -> B {
    let name = serde_json::to_value(body)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let other = if let Some(tail) = name.strip_prefix("left_") {
        format!("right_{tail}")
    } else if let Some(tail) = name.strip_prefix("right_") {
        format!("left_{tail}")
    } else {
        return body;
    };
    serde_json::from_value(serde_json::Value::String(other)).unwrap_or(body)
}
