use slimevr_core::{
    calibration::ResetKind,
    filtering::{FilterConfig, FilterType},
    pose::{PoseConfig, PoseEngine, TrackerBinding},
    skeleton::{BodyPosition, HeadPose, Skeleton, SkeletonConfig},
    EventKind, InputEvent, Quaternion as Q, SensorStatus, TrackerSample, Vector3 as V,
};
use std::collections::BTreeMap;

fn config() -> PoseConfig {
    PoseConfig {
        bindings: vec![TrackerBinding {
            device_key: "device".into(),
            sensor_id: 0,
            body: BodyPosition::Chest,
            mounting: Q::IDENTITY,
        }],
        filter: FilterConfig {
            mode: FilterType::None,
            amount: 0.0,
        },
        ..Default::default()
    }
}
fn sample(at: u64, q: Q, session: u64) -> TrackerSample {
    TrackerSample {
        source: "test".into(),
        device_key: "device".into(),
        sensor_id: 0,
        session,
        packet_sequence: at as i64,
        received_at_ms: at,
        sensor_timestamp_us: None,
        packet_rotation: None,
        server_rotation: Some(q),
        packet_acceleration: None,
        server_acceleration: None,
        position: None,
        compatibility_fallback: false,
    }
}

#[test]
fn full_reset_preserves_heading_and_rebases_attitude_then_yaw_preserves_tilt() {
    let mut engine = PoseEngine::new(config()).unwrap();
    engine
        .set_head(
            0,
            HeadPose {
                rotation: Q::rotation_y(0.6),
                position: Some(V {
                    x: 0.0,
                    y: 1.7,
                    z: 0.0,
                }),
            },
        )
        .unwrap();
    engine
        .sample(&sample(0, Q::rotation_x(0.9) * Q::rotation_z(0.3), 1))
        .unwrap();
    engine.reset(0, ResetKind::Full).unwrap();
    let first = engine.tick(0).unwrap();
    assert!(
        first.trackers[0]
            .rotation
            .unwrap()
            .angle_to_r(Q::rotation_y(0.6))
            < 2e-5
    );
    let raw = Q::rotation_x(1.1) * Q::rotation_z(0.3);
    engine.sample(&sample(1, raw, 1)).unwrap();
    let before = engine.tick(1).unwrap().trackers[0]
        .rotation
        .unwrap()
        .rotate(V::UP);
    engine
        .set_head(
            2,
            HeadPose {
                rotation: Q::rotation_y(-0.8),
                position: None,
            },
        )
        .unwrap();
    engine.reset(2, ResetKind::Yaw).unwrap();
    let after = engine.tick(2).unwrap().trackers[0]
        .rotation
        .unwrap()
        .rotate(V::UP);
    assert!(
        (before.y - after.y).abs() < 2e-5,
        "yaw must preserve inclination"
    );
}
#[test]
fn smoothing_counts_tick_time_since_last_sample_and_keeps_twins_continuous() {
    let mut cfg = config();
    cfg.filter = FilterConfig {
        mode: FilterType::Smoothing,
        amount: 1.0,
    };
    let mut engine = PoseEngine::new(cfg).unwrap();
    engine.sample(&sample(0, Q::IDENTITY, 1)).unwrap();
    engine.tick(0).unwrap();
    let target = Q::rotation_y(1.0);
    engine.sample(&sample(10, target, 1)).unwrap();
    let first = engine.tick(20).unwrap().trackers[0].rotation.unwrap();
    let second = engine.tick(40).unwrap().trackers[0].rotation.unwrap();
    assert!(second.angle_to_r(target) < first.angle_to_r(target));
    engine.sample(&sample(41, -target, 1)).unwrap();
    engine.tick(200).unwrap();
    assert!(engine.snapshot().trackers[0].rotation.unwrap().dot(target) > 0.99);
}
#[test]
fn parent_direction_is_global_and_does_not_rotate_child_direction_twice() {
    let c = SkeletonConfig {
        extended_spine: false,
        extended_pelvis: false,
        extended_knee: false,
        ..Default::default()
    };
    let inputs = BTreeMap::from([
        (
            BodyPosition::Chest,
            Q::rotation_z(std::f32::consts::FRAC_PI_2),
        ),
        (BodyPosition::Hip, Q::IDENTITY),
        (BodyPosition::LeftUpperLeg, Q::IDENTITY),
    ]);
    let out = Skeleton::new(c).solve(&inputs, None, false).unwrap();
    let thigh = out.bones["left_upper_leg"];
    assert!((thigh.tail.x - thigh.head.x).abs() < 1e-6);
    assert!((thigh.tail.y - thigh.head.y + c.upper_leg_length).abs() < 1e-6);
}
#[test]
fn missing_head_has_no_fabricated_world_anchor_or_head_neck_lengths() {
    let mut skeleton = Skeleton::new(SkeletonConfig::default());
    let out = skeleton.solve(&BTreeMap::new(), None, false).unwrap();
    assert!(!out.world_anchor_present);
    assert_eq!(out.bones["head"].length, 0.0);
    assert_eq!(out.bones["neck"].length, 0.0);
    let out = skeleton
        .solve(
            &BTreeMap::new(),
            Some(HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V {
                    x: 0.0,
                    y: 1.7,
                    z: 0.0,
                }),
            }),
            false,
        )
        .unwrap();
    assert!(out.world_anchor_present);
    assert!((out.bones["head"].tail.z - 0.1).abs() < 1e-6);
}
#[test]
fn reboot_clears_old_pose_filter_and_calibration_and_rejects_old_session() {
    let mut engine = PoseEngine::new(config()).unwrap();
    engine.sample(&sample(0, Q::rotation_x(0.5), 1)).unwrap();
    engine.reset(0, ResetKind::Full).unwrap();
    engine.tick(0).unwrap();
    engine
        .ingest(&InputEvent {
            at_ms: 10,
            kind: EventKind::DeviceConnected {
                device_key: "device".into(),
                address: "test".into(),
                firmware: None,
                session: 2,
                preserve_calibration: false,
            },
        })
        .unwrap();
    engine.sample(&sample(11, Q::IDENTITY, 1)).unwrap();
    let out = engine.tick(11).unwrap();
    assert_eq!(out.ignored_samples, 1);
    assert!(out.trackers[0].raw.is_none());
    assert!(!out.trackers[0].calibration.full_reset_done);
    engine.sample(&sample(12, Q::IDENTITY, 2)).unwrap();
    assert_eq!(engine.tick(12).unwrap().trackers[0].session, 2);
}

#[test]
fn reconnected_filters_start_at_the_first_fresh_calibrated_rotation() {
    for mode in [FilterType::Smoothing, FilterType::Prediction] {
        let mut cfg = config();
        cfg.filter.mode = mode;
        let mut engine = PoseEngine::new(cfg).unwrap();
        let initial = Q::rotation_x(0.8);
        engine.sample(&sample(0, initial, 1)).unwrap();
        engine.reset(0, ResetKind::Full).unwrap();
        engine
            .ingest(&InputEvent {
                at_ms: 3000,
                kind: EventKind::DeviceConnected {
                    device_key: "device".into(),
                    address: "test".into(),
                    firmware: None,
                    session: 2,
                    preserve_calibration: true,
                },
            })
            .unwrap();
        // It moved while the transport was unavailable; there is no valid filter history.
        engine.sample(&sample(3001, Q::rotation_x(1.1), 2)).unwrap();
        let pose = engine.tick(3002).unwrap();
        let tracker = &pose.trackers[0];
        assert!(tracker.calibration.full_reset_done);
        assert!(
            tracker
                .filtered
                .unwrap()
                .angle_to_r(tracker.calibrated.unwrap())
                < 1e-5,
            "{mode:?}"
        );
    }
}

#[test]
fn older_connected_events_still_request_a_fresh_calibration() {
    let event: InputEvent = serde_json::from_value(serde_json::json!({
        "at_ms": 10, "type": "device_connected", "device_key": "device",
        "address": "test", "firmware": null, "session": 2,
    }))
    .unwrap();
    let mut engine = PoseEngine::new(config()).unwrap();
    engine.sample(&sample(0, Q::rotation_x(0.5), 1)).unwrap();
    engine.reset(0, ResetKind::Full).unwrap();
    engine.ingest(&event).unwrap();
    assert!(
        !engine.tick(10).unwrap().trackers[0]
            .calibration
            .full_reset_done
    );
}
#[test]
fn heartbeat_and_acceleration_do_not_become_direction_samples_or_tick_twice() {
    let mut engine = PoseEngine::new(config()).unwrap();
    engine.sample(&sample(0, Q::IDENTITY, 1)).unwrap();
    engine.tick(0).unwrap();
    let mut a = sample(2000, Q::IDENTITY, 1);
    a.server_rotation = None;
    a.server_acceleration = Some(V {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    });
    engine.sample(&a).unwrap();
    let out = engine.tick(2500).unwrap();
    assert_eq!(out.trackers[0].pose_age_ms, Some(2500));
    assert_eq!(out.trackers[0].pose_stale, Some(true));
    assert_eq!(out.frame, 2);
    assert_eq!(
        out.trackers[0]
            .acceleration_world
            .as_ref()
            .unwrap()
            .received_at_ms,
        2000
    );
}
#[test]
fn delayed_reset_and_immediate_yaw_are_bounded_and_pause_preserves_body() {
    let mut engine = PoseEngine::new(config()).unwrap();
    engine.sample(&sample(0, Q::IDENTITY, 1)).unwrap();
    engine.action(0, 2).unwrap();
    engine.action(0, 3).unwrap();
    assert_eq!(engine.tick(0).unwrap().reset_count, 1);
    let before = engine.snapshot().skeleton.bones["chest"].rotation;
    engine.set_paused(1, true).unwrap();
    engine.sample(&sample(2, Q::rotation_x(0.8), 1)).unwrap();
    assert_eq!(
        engine.tick(2).unwrap().skeleton.bones["chest"].rotation,
        before
    );
    assert_eq!(engine.tick(3000).unwrap().reset_count, 2);
    engine.set_paused(3001, false).unwrap();
    engine.sample(&sample(3001, Q::rotation_x(1.0), 1)).unwrap();
    assert!(
        engine.tick(3001).unwrap().skeleton.bones["chest"]
            .rotation
            .angle_to_r(before)
            > 0.1
    );
}
#[test]
fn invalid_config_samples_and_backwards_clock_fail_explicitly() {
    let mut duplicate = config();
    duplicate.bindings.push(duplicate.bindings[0].clone());
    assert!(PoseEngine::new(duplicate).is_err());
    let mut bad = config();
    bad.skeleton.upper_leg_length = -1.0;
    assert!(PoseEngine::new(bad).is_err());
    let mut engine = PoseEngine::new(config()).unwrap();
    assert!(engine.sample(&sample(0, Q::ZERO, 1)).is_err());
    engine.tick(100).unwrap();
    assert!(engine.tick(99).is_err());
    assert!(engine
        .set_head(
            100,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V {
                    x: f32::NAN,
                    y: 0.0,
                    z: 0.0
                })
            }
        )
        .is_err());
}
#[test]
fn disconnected_source_falls_back_without_deleting_last_observed_data() {
    let mut engine = PoseEngine::new(config()).unwrap();
    engine
        .set_head(
            0,
            HeadPose {
                rotation: Q::IDENTITY,
                position: None,
            },
        )
        .unwrap();
    engine.sample(&sample(0, Q::rotation_x(0.5), 1)).unwrap();
    engine.tick(0).unwrap();
    engine
        .ingest(&InputEvent {
            at_ms: 1,
            kind: EventKind::SensorState {
                device_key: "device".into(),
                sensor_id: 0,
                status: SensorStatus::Disconnected,
            },
        })
        .unwrap();
    let out = engine.tick(1).unwrap();
    assert!(out.trackers[0].raw.is_some());
    assert!(out.skeleton.bones["chest"].rotation.angle_to_r(Q::IDENTITY) < 1e-6);
}

#[test]
fn mounting_reset_is_exported_and_restored_after_device_restart() {
    let mut c = config();
    c.save_mounting_reset = true;
    let mut engine = PoseEngine::new(c).unwrap();
    engine.sample(&sample(0, Q::IDENTITY, 1)).unwrap();
    engine.reset(0, ResetKind::Full).unwrap();
    engine
        .sample(&sample(1, Q::rotation_x(0.6) * Q::rotation_z(0.3), 1))
        .unwrap();
    engine.reset(1, ResetKind::Mounting).unwrap();
    let saved = engine.export_config();
    let fix = saved.saved_mounting_resets[&BodyPosition::Chest];
    assert!(fix.angle_to_r(Q::IDENTITY) > 0.1);
    let persisted: PoseConfig =
        serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
    let mut restored = PoseEngine::new(persisted).unwrap();
    restored.sample(&sample(2, Q::IDENTITY, 1)).unwrap();
    assert_eq!(
        restored.tick(2).unwrap().trackers[0]
            .calibration
            .mount_rot_fix,
        fix
    );
    engine
        .ingest(&InputEvent {
            at_ms: 3,
            kind: EventKind::DeviceConnected {
                device_key: "device".into(),
                address: "new".into(),
                firmware: None,
                session: 2,
                preserve_calibration: false,
            },
        })
        .unwrap();
    assert_eq!(
        engine.tick(3).unwrap().trackers[0]
            .calibration
            .mount_rot_fix,
        fix
    );
    assert!(!engine.snapshot().trackers[0].calibration.full_reset_done);
}

#[test]
fn sensor_metadata_routes_flex_then_disconnect_excludes_it_and_rotation_clears_it() {
    let mut c = config();
    c.bindings[0].body = BodyPosition::LeftIndexProximal;
    let mut engine = PoseEngine::new(c).unwrap();
    let metadata = |at, data_type| InputEvent {
        at_ms: at,
        kind: EventKind::SensorMetadata {
            device_key: "device".into(),
            sensor_id: 0,
            imu_type: 13,
            data_type,
            magnetometer_enabled: false,
        },
    };
    engine.ingest(&metadata(0, 2)).unwrap();
    engine
        .ingest(&InputEvent {
            at_ms: 0,
            kind: EventKind::SensorRegistered {
                device_key: "device".into(),
                sensor_id: 0,
                status: SensorStatus::Ok,
            },
        })
        .unwrap();
    engine
        .ingest(&InputEvent {
            at_ms: 1,
            kind: EventKind::FlexValue {
                device_key: "device".into(),
                sensor_id: 0,
                session: 1,
                value: 0.6,
            },
        })
        .unwrap();
    let pose = engine.tick(1).unwrap();
    assert!(
        pose.skeleton.bones["left_index_proximal"]
            .rotation
            .angle_to_r(Q::IDENTITY)
            > 0.5
    );
    assert_eq!(
        engine.export_config().imu_types[&BodyPosition::LeftIndexProximal],
        13
    );
    engine
        .ingest(&InputEvent {
            at_ms: 2,
            kind: EventKind::SensorState {
                device_key: "device".into(),
                sensor_id: 0,
                status: SensorStatus::Disconnected,
            },
        })
        .unwrap();
    assert!(
        engine.tick(2).unwrap().skeleton.bones["left_index_proximal"]
            .rotation
            .angle_to_r(Q::IDENTITY)
            < 1e-5
    );
    engine.ingest(&metadata(3, 0)).unwrap();
    assert!(engine.export_config().flex_angles.is_empty());
    assert!(engine.tick(3).unwrap().flex_rotations.is_empty());
}

#[test]
fn positioned_head_sample_anchors_pose_and_external_sources_can_be_removed() {
    let mut c = config();
    c.bindings[0].body = BodyPosition::Head;
    c.skeleton.force_arms_from_hmd = false;
    let mut engine = PoseEngine::new(c).unwrap();
    let mut s = sample(0, Q::IDENTITY, 1);
    s.position = Some(V::new(0.2, 1.7, -0.1));
    engine.sample(&s).unwrap();
    let p = engine.tick(0).unwrap();
    assert!(p.skeleton.world_anchor_present);
    assert_eq!(p.skeleton.bones["head"].head, s.position.unwrap());
    assert_eq!(engine.motion_frame().unwrap().head.position, s.position);
    engine
        .set_head(
            1,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.0, 2.0, 0.0)),
            },
        )
        .unwrap();
    engine
        .set_controller(
            1,
            BodyPosition::LeftHand,
            Q::IDENTITY,
            V::new(-0.4, 1.0, -0.2),
        )
        .unwrap();
    assert_eq!(
        engine.tick(1).unwrap().skeleton.computed["left_hand"].position,
        V::new(-0.4, 1.0, -0.2)
    );
    engine.clear_head(2).unwrap();
    engine.clear_controller(2, BodyPosition::LeftHand).unwrap();
    let p = engine.tick(2).unwrap();
    assert_eq!(p.skeleton.bones["head"].head, s.position.unwrap());
    assert_ne!(
        p.skeleton.computed["left_hand"].position,
        V::new(-0.4, 1.0, -0.2)
    );
}

#[test]
fn live_configuration_preserves_calibration_and_received_sample_age() {
    let mut e = PoseEngine::new(config()).unwrap();
    e.sample(&sample(0, Q::rotation_x(0.7), 1)).unwrap();
    e.reset(0, ResetKind::Full).unwrap();
    e.tick(0).unwrap();
    let calibration = serde_json::to_value(&e.snapshot().trackers[0].calibration).unwrap();
    let mut changed = e.export_config();
    changed.filter.mode = FilterType::Smoothing;
    changed.filter.amount = 0.3;
    changed.skeleton.hips_width = 0.3;
    e.configure(100, changed).unwrap();
    let p = e.tick(100).unwrap();
    assert_eq!(p.trackers[0].pose_age_ms, Some(100));
    assert_eq!(p.reset_count, 1);
    assert_eq!(
        serde_json::to_value(&p.trackers[0].calibration).unwrap(),
        calibration
    );
    assert!(p.trackers[0].rotation.unwrap().angle_to_r(Q::IDENTITY) < 2e-5);
}

#[test]
fn binding_an_existing_sample_retains_its_original_observation_clock() {
    let mut e = PoseEngine::new(config()).unwrap();
    e.tick(100).unwrap();
    e.restore_sample(150, &sample(20, Q::rotation_z(0.4), 1))
        .unwrap();
    let p = e.tick(150).unwrap();
    assert_eq!(p.trackers[0].pose_age_ms, Some(130));
    assert_eq!(p.trackers[0].raw.as_ref().unwrap().received_at_ms, 20);
    assert!(e.restore_sample(160, &sample(170, Q::IDENTITY, 1)).is_err());
}

#[test]
fn selected_reset_and_temporary_leg_settings_do_not_modify_other_trackers_or_saved_settings() {
    let mut c = config();
    c.bindings.push(TrackerBinding {
        device_key: "device".into(),
        sensor_id: 1,
        body: BodyPosition::Hip,
        mounting: Q::IDENTITY,
    });
    let mut e = PoseEngine::new(c).unwrap();
    e.sample(&sample(0, Q::rotation_x(0.7), 1)).unwrap();
    let mut second = sample(0, Q::rotation_z(0.4), 1);
    second.sensor_id = 1;
    e.sample(&second).unwrap();
    e.reset_selected(0, ResetKind::Full, &[BodyPosition::Chest].into())
        .unwrap();
    let p = e.tick(0).unwrap();
    assert!(p.trackers[0].calibration.full_reset_done);
    assert!(!p.trackers[1].calibration.full_reset_done);
    let saved = e.export_config().legs;
    e.set_leg_overrides(1, [Some(false); 4]).unwrap();
    assert_eq!(e.export_config().legs, saved);
    let mut updated = e.export_config();
    updated.legs.skating = false;
    e.configure(2, updated).unwrap();
    assert_eq!(e.leg_overrides(), [Some(false); 4]);
    e.set_leg_overrides(3, [None; 4]).unwrap();
    assert!(!e.export_config().legs.skating);
    e.set_paused(4, true).unwrap();
    assert!(e.is_paused());
}

#[test]
fn external_sources_clear_positions_on_rotation_only_timeout_and_clear_mounting_replays() {
    use slimevr_core::pose::SceneInput;
    let mut c = config();
    c.bindings[0].body = BodyPosition::Head;
    let mut engine = PoseEngine::new(c).unwrap();
    engine
        .ingest(&InputEvent {
            at_ms: 0,
            kind: EventKind::ExternalTracker {
                device_key: "device".into(),
                sensor_id: 0,
                source: "test".into(),
                name: "head".into(),
                body: Some(BodyPosition::Head),
                capabilities: slimevr_core::TrackerCapabilities {
                    allow_reset: true,
                    allow_mounting: false,
                    allow_filter: false,
                    is_imu: false,
                },
            },
        })
        .unwrap();
    let mut observation = sample(1, Q::IDENTITY, 1);
    observation.position = Some(V::new(0.0, 1.7, 0.0));
    engine.sample(&observation).unwrap();
    assert!(engine.tick(1).unwrap().skeleton.world_anchor_present);
    observation = sample(2, Q::rotation_x(0.2), 1);
    engine.sample(&observation).unwrap();
    assert!(!engine.tick(2).unwrap().skeleton.world_anchor_present);
    observation.position = Some(V::new(0.0, 1.7, 0.0));
    observation.received_at_ms = 3;
    engine.sample(&observation).unwrap();
    engine
        .ingest(&InputEvent {
            at_ms: 4,
            kind: EventKind::SensorState {
                device_key: "device".into(),
                sensor_id: 0,
                status: SensorStatus::TimedOut,
            },
        })
        .unwrap();
    assert!(!engine.tick(4).unwrap().skeleton.world_anchor_present);
    assert!(engine.motion_frame().is_err());
    engine
        .scene_input(SceneInput::ClearMounting { at_ms: 5 })
        .unwrap();
    let p = engine.tick(5).unwrap();
    assert!(!p.mounting_completed);
    assert!(!p.feet_mounting_completed);
}
