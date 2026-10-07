//! Canonical FK snapshots for differential testing of the two BVH exporters.
use slimevr_core::{
    skeleton::{BodyPosition as B, HeadPose, Skeleton, SkeletonConfig},
    Quaternion as Q, Vector3 as V,
};
use std::collections::BTreeMap;
fn main() {
    let mut cases = Vec::new();
    for mode in 0..6 {
        let config = SkeletonConfig {
            force_arms_from_hmd: mode < 2,
            skeleton_offset: 0.035,
            chest_offset: 0.025,
            hip_offset: 0.01,
            elbow_offset: 0.018,
            enforce_constraints: mode == 1,
            ..Default::default()
        };
        let mut skeleton = Skeleton::new(config);
        let mut poses = Vec::new();
        for i in 0..8 {
            let mut inputs = BTreeMap::new();
            for (j, body) in [
                B::Chest,
                B::Hip,
                B::LeftUpperLeg,
                B::RightUpperLeg,
                B::LeftLowerLeg,
                B::RightLowerLeg,
                B::LeftUpperArm,
                B::RightLowerArm,
                B::LeftHand,
                B::RightHand,
                B::LeftIndexProximal,
                B::RightThumbDistal,
            ]
            .into_iter()
            .enumerate()
            {
                let q = Q::rotation_z((i as f32 - 3.0) * 0.13 + j as f32 * 0.04)
                    * Q::rotation_x((i as f32 - j as f32) * 0.12)
                    * Q::rotation_y(j as f32 * 0.15);
                inputs.insert(body, if i % 3 == 2 { -q } else { q });
            }
            let mut positions = BTreeMap::new();
            if mode >= 2 {
                positions.insert(B::LeftHand, V::new(-0.3, 1.2, -0.4));
            }
            if mode >= 3 {
                positions.insert(B::RightHand, V::new(0.4, 1.1, -0.3));
            }
            let head = (mode != 4).then_some(HeadPose {
                rotation: Q::rotation_y(i as f32 * 0.14),
                position: Some(V::new(i as f32 * 0.1, 1.7, -0.1)),
            });
            let mut pose = skeleton
                .solve_with_positions(&inputs, &positions, head, false)
                .unwrap();
            if mode == 5 {
                let hip = pose.bones.get_mut("hip").unwrap();
                hip.rotation = hip.rotation_offset
                    * Q::rotation_z(0.6)
                    * Q::rotation_x(if i % 2 == 0 {
                        std::f32::consts::FRAC_PI_2
                    } else {
                        -std::f32::consts::FRAC_PI_2
                    })
                    * Q::rotation_y(-0.3);
                hip.tail = hip.head + hip.rotation.rotate(V::DOWN) * hip.length;
            }
            poses.push(pose);
        }
        cases.push(
            serde_json::json!({"name":format!("layout_{mode}"), "poses":poses,
            "left_controller":mode>=2, "right_controller":mode>=3}),
        );
    }
    println!("{}", serde_json::json!({"cases":cases}));
}
