use slimevr_core::{
    alignment::{AlignmentConfig, RestDetector, RestState, StayAligned},
    autobone::{self, AutoBoneConfig, MotionFrame},
    calibration::ResetKind,
    flex::FlexSensor,
    gestures::{HeightCalibration, HeightStatus, TapDetector},
    legs::{LegConfig, LegTweaks},
    pose::{PoseConfig, PoseEngine},
    skeleton::{BodyPosition as B, HeadPose, Skeleton, SkeletonConfig},
    Quaternion as Q, Vector3 as V,
};
use std::collections::BTreeMap;
fn six() -> BTreeMap<B, Q> {
    [
        B::Chest,
        B::Hip,
        B::LeftUpperLeg,
        B::RightUpperLeg,
        B::LeftLowerLeg,
        B::RightLowerLeg,
    ]
    .into_iter()
    .map(|b| (b, Q::IDENTITY))
    .collect()
}
#[test]
fn controller_roots_reverse_forearms_and_constraints_do_not_move_controller_anchors() {
    let c = SkeletonConfig {
        force_arms_from_hmd: false,
        enforce_constraints: true,
        ..Default::default()
    };
    let inputs = BTreeMap::from([
        (B::LeftLowerArm, Q::IDENTITY),
        (B::LeftHand, Q::rotation_y(0.4)),
    ]);
    let pos = V::new(-0.3, 1.1, -0.4);
    let p = Skeleton::new(c)
        .solve_with_positions(
            &inputs,
            &BTreeMap::from([(B::LeftHand, pos)]),
            Some(HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.0, 1.7, 0.0)),
            }),
            false,
        )
        .unwrap();
    assert!((p.computed["left_hand"].position - pos).len() < 1e-6);
    assert!(
        (p.bones["left_lower_arm"].tail.y - p.bones["left_lower_arm"].head.y - c.lower_arm_length)
            .abs()
            < 1e-6
    );
    assert!((p.bones["left_thumb_metacarpal"].head - p.bones["left_hand"].tail).len() < 1e-6);
}
#[test]
fn rest_lock_requires_continuous_quiet_then_holds_recently_rest_for_three_seconds() {
    let mut rest = RestDetector::new(0);
    rest.update(1000, Q::IDENTITY);
    assert_eq!(rest.state, RestState::Moving);
    rest.update(1001, Q::IDENTITY);
    assert_eq!(rest.state, RestState::AtRest);
    rest.update(1100, Q::rotation_x(0.1));
    assert_eq!(rest.state, RestState::RecentlyAtRest);
    for at in [1800, 2500, 3200, 4100] {
        rest.update(at, Q::rotation_x(at as f32 / 1000.0));
        assert_eq!(rest.state, RestState::RecentlyAtRest);
    }
    rest.update(4101, Q::rotation_x(4.2));
    assert_eq!(rest.state, RestState::Moving);
    let mut aligned = StayAligned::new(AlignmentConfig {
        enabled: true,
        ..Default::default()
    });
    aligned.update_rest(0, B::Hip, Q::IDENTITY, Q::IDENTITY);
    aligned.update_rest(1001, B::Hip, Q::IDENTITY, Q::IDENTITY);
    aligned.reset();
    assert!(aligned.states.is_empty());
}
#[test]
fn foot_history_is_bounded_through_long_walk_and_reset() {
    let mut legs = LegTweaks::new(LegConfig {
        floor_clip: true,
        skating: true,
        foot_plant: true,
        toe_snap: true,
        ..Default::default()
    });
    legs.reset(true);
    let mut skeleton = Skeleton::new(SkeletonConfig::default());
    let mut inputs = six();
    for i in 1..=4000 {
        let t = i as f32 * 0.015;
        inputs.insert(B::LeftUpperLeg, Q::rotation_x(-0.4 * t.sin()));
        inputs.insert(B::LeftLowerLeg, Q::rotation_x(0.5 * t.sin()));
        let mut p = skeleton
            .solve(
                &inputs,
                Some(HeadPose {
                    rotation: Q::IDENTITY,
                    position: Some(V::new(0.0, 1.58, 0.0)),
                }),
                false,
            )
            .unwrap();
        legs.update(i * 4, &mut p, &inputs, &BTreeMap::new())
            .unwrap();
        assert!(legs.history.len() <= 11);
        assert!(p
            .computed
            .values()
            .all(|p| p.position.is_finite() && p.rotation.is_rotation()));
    }
    legs.reset(false);
    assert!(legs.history.is_empty());
}
#[test]
fn flex_resistance_can_reverse_and_partial_finger_chain_receives_relative_hand_rotation() {
    let mut flex = FlexSensor::default();
    flex.resistance(B::LeftIndexProximal, 100.0).unwrap();
    flex.reset_min(B::LeftIndexProximal);
    flex.resistance(B::LeftIndexProximal, 10.0).unwrap();
    flex.reset_max(B::LeftIndexProximal);
    let q = flex.resistance(B::LeftIndexProximal, 55.0).unwrap();
    assert!(q.angle_to_r(Q::rotation_z(std::f32::consts::FRAC_PI_4)) < 1e-5);
    let mut config = PoseConfig::default();
    config.flex_angles.insert(B::LeftIndexProximal);
    let mut engine = PoseEngine::new(config).unwrap();
    engine
        .set_controller(0, B::LeftHand, Q::rotation_y(0.3), V::ZERO)
        .unwrap();
    engine.tick(0).unwrap();
    engine.set_flex(4, B::LeftIndexProximal, 0.4).unwrap();
    let p = engine.tick(4).unwrap();
    assert!(
        p.skeleton.bones["left_index_proximal"]
            .rotation
            .angle_to_r(Q::rotation_y(0.3) * Q::rotation_z(0.4))
            < 1e-5
    );
}
#[test]
fn tap_requires_separate_peaks_and_rejects_body_motion() {
    let mut tap = TapDetector::default();
    let mut a = BTreeMap::from([(B::Chest, V::ZERO)]);
    assert!(!tap.update(0, B::Chest, &a, 2, 1));
    a.insert(B::Chest, V::new(8.0, 0.0, 0.0));
    assert!(!tap.update(4, B::Chest, &a, 2, 1));
    a.insert(B::Chest, V::ZERO);
    assert!(!tap.update(80, B::Chest, &a, 2, 1));
    a.insert(B::Chest, V::new(8.0, 0.0, 0.0));
    assert!(tap.update(84, B::Chest, &a, 2, 1));
    a.insert(B::Hip, V::new(3.0, 0.0, 0.0));
    for at in [88, 160, 164] {
        assert!(!tap.update(at, B::Chest, &a, 2, 1));
    }
}
#[test]
fn height_calibration_requires_downward_controller_stability_and_forward_hmd() {
    let mut h = HeightCalibration::default();
    h.start(0);
    let head = HeadPose {
        rotation: Q::IDENTITY,
        position: Some(V::new(0.0, 1.72, 0.0)),
    };
    let hands = BTreeMap::from([(
        B::LeftHand,
        (
            Q::rotation_x(-std::f32::consts::FRAC_PI_2),
            V::new(0.0, 0.02, 0.0),
        ),
    )]);
    for i in 0..180 {
        h.update(i * 4, Some(head), &hands);
    }
    assert_eq!(h.status, HeightStatus::RecordingHeight);
    let mut measured = None;
    for i in 180..=500 {
        measured = h.update(i * 4, Some(head), &hands).or(measured);
    }
    assert_eq!(h.status, HeightStatus::Done);
    assert!((measured.unwrap() - 1.7).abs() < 1e-5);
    h.start(0);
    h.update(30001, None, &BTreeMap::new());
    assert_eq!(h.status, HeightStatus::Timeout);
}
#[test]
fn localizer_has_no_fabricated_hmd_neck_lengths_and_preserves_external_anchor() {
    let mut c = PoseConfig::default();
    c.localizer.enabled = true;
    let mut engine = PoseEngine::new(c).unwrap();
    let p = engine.tick(4).unwrap();
    assert_eq!(p.skeleton.bones["neck"].length, 0.0);
    assert!(!p.skeleton.world_anchor_present);
    engine
        .set_head(
            8,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.2, 1.7, 0.1)),
            },
        )
        .unwrap();
    let p = engine.tick(8).unwrap();
    assert!(p.skeleton.world_anchor_present);
    assert_eq!(p.skeleton.bones["head"].head, V::new(0.2, 1.7, 0.1));
    engine.reset(8, ResetKind::Full).unwrap();
}
#[test]
fn autobone_reduces_stationary_ankle_error_preserves_height_and_repeats_identically() {
    let truth = SkeletonConfig {
        enforce_constraints: false,
        extended_knee: false,
        upper_leg_length: 0.46,
        lower_leg_length: 0.54,
        waist_length: 0.24,
        ..Default::default()
    };
    let height = autobone::skeleton_height(truth);
    let mut frames = Vec::new();
    for i in 0..48 {
        let t = i as f32 * 0.17;
        let mut inputs = six();
        inputs.insert(B::Chest, Q::rotation_x(t.sin() * 0.25));
        inputs.insert(B::Hip, Q::rotation_x(0.0));
        for b in [B::LeftUpperLeg, B::RightUpperLeg] {
            inputs.insert(b, Q::rotation_x(-0.6 * t.sin().abs()));
        }
        for b in [B::LeftLowerLeg, B::RightLowerLeg] {
            inputs.insert(b, Q::rotation_x(0.5 * t.sin().abs()));
        }
        let mut skeleton = Skeleton::new(truth);
        let p = skeleton
            .solve(
                &inputs,
                Some(HeadPose {
                    rotation: Q::IDENTITY,
                    position: Some(V::ZERO),
                }),
                false,
            )
            .unwrap();
        let ankle = p.bones["left_lower_leg"].tail;
        let head = HeadPose {
            rotation: Q::IDENTITY,
            position: Some(V::new(0.0, -ankle.y, -ankle.z)),
        };
        frames.push(MotionFrame {
            at_ms: i * 20,
            rotations: inputs,
            head,
            positions: BTreeMap::new(),
        });
    }
    let cfg = AutoBoneConfig {
        epochs: 12,
        proportion_factor: 0.0,
        max_final_error: 0.5,
        ..Default::default()
    };
    let initial = SkeletonConfig {
        enforce_constraints: false,
        extended_knee: false,
        ..Default::default()
    };
    let a = autobone::optimize(initial, &frames, Some(height), cfg).unwrap();
    assert!(
        a.evaluation_error < a.initial_error,
        "{} -> {}",
        a.initial_error,
        a.evaluation_error
    );
    assert!(a.accepted);
    assert!((autobone::skeleton_height(a.skeleton) - height).abs() < 1e-5);
    let b = autobone::optimize(initial, &frames, Some(height), cfg).unwrap();
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
    assert!(autobone::optimize(initial, &frames[..1], Some(height), cfg).is_err());
}

#[test]
fn completed_height_measurement_applies_proportions_and_exports_reloadable_configuration() {
    let mut c = PoseConfig::default();
    c.skeleton.hips_width = 0.4;
    c.skeleton.upper_leg_length = 0.7;
    c.skeleton.extended_knee = false;
    let mut engine = PoseEngine::new(c).unwrap();
    engine
        .set_head(
            0,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.0, 1.72, 0.0)),
            },
        )
        .unwrap();
    engine
        .set_controller(
            0,
            B::LeftHand,
            Q::rotation_x(-std::f32::consts::FRAC_PI_2),
            V::new(0.0, 0.02, 0.0),
        )
        .unwrap();
    engine.start_height_calibration(0).unwrap();
    for i in 0..=500 {
        engine.tick(i * 4).unwrap();
    }
    assert_eq!(engine.snapshot().height_status, HeightStatus::Done);
    let c = engine.export_config();
    assert!((c.hmd_height.unwrap() - 1.7).abs() < 1e-5);
    assert!((autobone::skeleton_height(c.skeleton) - 1.7).abs() < 1e-5);
    assert!((c.skeleton.upper_arm_length - 1.7 * (0.26 / 1.58)).abs() < 1e-5);
    assert_eq!(c.skeleton.hips_width, SkeletonConfig::default().hips_width);
    assert!(!c.skeleton.extended_knee);
    PoseEngine::new(serde_json::from_value(serde_json::to_value(c).unwrap()).unwrap()).unwrap();
    assert!(engine.apply_height(2000, f32::NAN).is_err());
}
