use slimevr_core::{
    calibration::ResetKind,
    pose::{PoseConfig, PoseEngine},
    skeleton::HeadPose,
    velocity::DerivedVelocity,
    Quaternion as Q, Vector3 as V,
};

fn head(engine: &mut PoseEngine, at: u64, position: V) {
    engine
        .set_head(
            at,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(position),
            },
        )
        .unwrap();
}
fn engine() -> PoseEngine {
    let mut config = PoseConfig {
        send_derived_velocity: true,
        ..Default::default()
    };
    config.legs.enabled = false;
    PoseEngine::new(config).unwrap()
}
fn close(actual: V, expected: V) {
    assert!(
        (actual - expected).len() < 0.001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn invalid_observations_overflow_and_reversed_clocks_never_publish_invalid_velocity() {
    let mut estimator = DerivedVelocity::default();
    assert_eq!(estimator.update(1000, Some(V::ZERO)), None);
    assert_eq!(estimator.update(999, Some(V::new(1.0, 2.0, 3.0))), None);
    close(
        estimator
            .update(1999, Some(V::new(1.01, 2.02, 3.03)))
            .unwrap(),
        V::new(10.0, 20.0, 30.0),
    );
    assert_eq!(
        estimator.update(2000, Some(V::new(f32::NAN, 0.0, 0.0))),
        None
    );
    assert_eq!(estimator.update(3000, Some(V::ZERO)), None);
    assert_eq!(
        estimator.update(4000, Some(V::new(f32::MAX, 0.0, 0.0))),
        None
    );
    assert_eq!(
        estimator.update(5000, Some(V::new(-f32::MAX, 0.0, 0.0))),
        None
    );
    estimator.update(6000, None);
    assert_eq!(estimator.update(7000, Some(V::ZERO)), None);
    close(
        estimator
            .update(8000, Some(V::new(0.01, 0.0, 0.0)))
            .unwrap(),
        V::new(10.0, 0.0, 0.0),
    );
}

#[test]
fn computed_velocity_tracks_translation_and_uses_post_leg_tweaks_positions() {
    let mut engine = engine();
    let mut config = engine.export_config();
    config.legs.enabled = true;
    config.legs.always_use_floor_clip = true;
    config.legs.skating = false;
    config.legs.foot_plant = false;
    config.legs.toe_snap = false;
    engine.configure(0, config).unwrap();
    head(&mut engine, 0, V::new(0.0, 1.7, 0.0));
    engine.reset(0, ResetKind::Full).unwrap();
    let first = engine.tick(0).unwrap().clone();
    assert!(first.computed_velocities.is_empty());
    head(&mut engine, 20, V::new(0.02, 1.6, -0.04));
    let second = engine.tick(20).unwrap();
    assert_eq!(second.computed_velocities.len(), 11);
    for (name, velocity) in &second.computed_velocities {
        close(
            *velocity,
            (second.skeleton.computed[name].position - first.skeleton.computed[name].position)
                / 0.02,
        );
    }
    close(second.computed_velocities["head"], V::new(1.0, -5.0, -2.0));
    let fk_velocity = (second.skeleton.bones["left_foot_tracker"].tail
        - first.skeleton.bones["left_foot_tracker"].tail)
        / 0.02;
    assert!(
        (second.computed_velocities["left_foot"] - fk_velocity).len() > 1.0,
        "clip must distinguish final position from FK"
    );
}

#[test]
fn velocity_is_invalidated_on_reset_pause_source_loss_and_shape_changes() {
    let mut engine = engine();
    head(&mut engine, 0, V::new(0.0, 1.7, 0.0));
    engine.tick(0).unwrap();
    head(&mut engine, 4, V::new(0.01, 1.7, 0.0));
    assert!(!engine.tick(4).unwrap().computed_velocities.is_empty());
    for (at, kind) in [
        (8, ResetKind::Full),
        (16, ResetKind::Yaw),
        (24, ResetKind::Mounting),
    ] {
        engine.reset(at, kind).unwrap();
        assert!(engine.tick(at).unwrap().computed_velocities.is_empty());
        assert!(!engine.tick(at + 4).unwrap().computed_velocities.is_empty());
    }
    engine.set_paused(32, true).unwrap();
    assert!(engine.tick(32).unwrap().computed_velocities.is_empty());
    engine.set_paused(36, false).unwrap();
    assert!(engine.tick(36).unwrap().computed_velocities.is_empty());
    assert!(!engine.tick(40).unwrap().computed_velocities.is_empty());
    engine.clear_head(41).unwrap();
    head(&mut engine, 42, V::new(5.0, 1.7, 0.0));
    assert!(
        engine.tick(44).unwrap().computed_velocities.is_empty(),
        "reconnect between ticks must clear baseline"
    );
    assert!(!engine.tick(48).unwrap().computed_velocities.is_empty());
    engine.clear_head(49).unwrap();
    assert!(engine.tick(52).unwrap().computed_velocities.is_empty());
    head(&mut engine, 53, V::new(9.0, 1.7, 0.0));
    assert!(engine.tick(56).unwrap().computed_velocities.is_empty());
    assert!(!engine.tick(60).unwrap().computed_velocities.is_empty());
    let mut config = engine.export_config();
    config.skeleton.hips_width += 0.1;
    engine.configure(61, config).unwrap();
    assert!(engine.tick(64).unwrap().computed_velocities.is_empty());
    assert!(!engine.tick(68).unwrap().computed_velocities.is_empty());
    engine.apply_height(69, 1.6).unwrap();
    assert!(engine.tick(72).unwrap().computed_velocities.is_empty());
}

#[test]
fn disabled_relative_and_gap_frames_seed_fresh_history_when_reenabled() {
    let mut engine = engine();
    assert!(engine.tick(0).unwrap().computed_velocities.is_empty());
    head(&mut engine, 4, V::new(0.0, 1.7, 0.0));
    assert!(engine.tick(4).unwrap().computed_velocities.is_empty());
    assert!(!engine.tick(8).unwrap().computed_velocities.is_empty());
    let mut config = engine.export_config();
    config.send_derived_velocity = false;
    engine.configure(9, config.clone()).unwrap();
    assert!(engine.tick(12).unwrap().computed_velocities.is_empty());
    head(&mut engine, 16, V::new(100.0, 1.7, 0.0));
    config.send_derived_velocity = true;
    engine.configure(16, config).unwrap();
    assert!(engine.tick(16).unwrap().computed_velocities.is_empty());
    close(
        engine.tick(20).unwrap().computed_velocities["head"],
        V::ZERO,
    );
    assert!(engine.tick(20).unwrap().computed_velocities.is_empty());
    assert!(!engine.tick(270).unwrap().computed_velocities.is_empty());
    assert!(engine.tick(521).unwrap().computed_velocities.is_empty());
    close(
        engine.tick(525).unwrap().computed_velocities["head"],
        V::ZERO,
    );
}
