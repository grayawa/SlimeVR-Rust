use slimevr_core::{
    calibration::ResetKind,
    filtering::FilterType,
    pose::{PoseConfig, PoseEngine, TrackerBinding},
    skeleton::{BodyPosition as B, HeadPose},
    Quaternion as Q, TrackerSample, Vector3 as V,
};
use std::collections::BTreeSet;
fn sample(body_id: u8, at: u64, q: Q) -> TrackerSample {
    TrackerSample {
        source: "test".into(),
        device_key: "d".into(),
        sensor_id: body_id,
        session: 1,
        packet_sequence: at as i64,
        received_at_ms: at,
        sensor_timestamp_us: None,
        packet_rotation: Some(q),
        server_rotation: Some(q),
        packet_acceleration: None,
        server_acceleration: None,
        position: None,
        compatibility_fallback: false,
    }
}
fn config() -> PoseConfig {
    let mut c = PoseConfig::default();
    c.filter.mode = FilterType::None;
    c.legs.enabled = false;
    c.bindings = [B::Chest, B::LeftFoot, B::LeftIndexProximal]
        .into_iter()
        .enumerate()
        .map(|(i, body)| TrackerBinding {
            device_key: "d".into(),
            sensor_id: i as u8,
            body,
            mounting: Q::IDENTITY,
        })
        .collect();
    c
}
#[test]
fn hmd_pitch_reset_applies_to_future_samples_recording_and_selected_full_resets() {
    let mut c = config();
    c.reset_hmd_pitch = true;
    let mut e = PoseEngine::new(c).unwrap();
    let head = |q| HeadPose {
        rotation: q,
        position: Some(V::new(1., 1.7, 2.)),
    };
    let raw = Q::rotation_y(0.8) * Q::rotation_x(0.4);
    e.set_head(0, head(raw)).unwrap();
    e.sample(&sample(0, 0, Q::IDENTITY)).unwrap();
    e.reset(0, ResetKind::Full).unwrap();
    let out = e.tick(0).unwrap();
    assert!(
        out.skeleton.computed["head"]
            .rotation
            .angle_to_r(Q::rotation_y(0.8))
            < 1e-5
    );
    assert_eq!(out.skeleton.bones["head"].head, head(raw).position.unwrap());
    assert!(
        e.motion_frame()
            .unwrap()
            .head
            .rotation
            .angle_to_r(Q::rotation_y(0.8))
            < 1e-5
    );
    e.set_head(4, head(Q::rotation_y(-0.3) * Q::rotation_x(0.6)))
        .unwrap();
    assert!(
        e.tick(4).unwrap().skeleton.computed["head"]
            .rotation
            .angle_to_r(Q::rotation_y(-0.3) * Q::rotation_x(0.2))
            < 1e-5
    );
    let correction = e.calibrated_head().unwrap().rotation;
    e.reset_selected(5, ResetKind::Full, &BTreeSet::from([B::Chest]))
        .unwrap();
    assert_eq!(e.calibrated_head().unwrap().rotation, correction);
    e.reset_selected(6, ResetKind::Full, &BTreeSet::from([B::Head]))
        .unwrap();
    assert!(
        e.calibrated_head()
            .unwrap()
            .rotation
            .angle_to_r(Q::rotation_y(-0.3))
            < 1e-5
    );
    e.clear_head(7).unwrap();
    e.set_head(8, head(raw)).unwrap();
    assert_eq!(e.calibrated_head().unwrap().rotation, raw);
}
#[test]
fn default_mounting_excludes_fingers_and_optional_feet_explicit_selection_overrides() {
    let mut e = PoseEngine::new(config()).unwrap();
    for i in 0..3 {
        e.sample(&sample(i, 0, Q::IDENTITY)).unwrap();
    }
    e.reset(0, ResetKind::Full).unwrap();
    for i in 0..3 {
        e.sample(&sample(i, 1, Q::rotation_x(0.7))).unwrap();
    }
    e.reset(1, ResetKind::Mounting).unwrap();
    let p = e.tick(1).unwrap();
    assert!(p.trackers[0].calibration.mounting_reset_done);
    assert!(!p.trackers[1].calibration.mounting_reset_done);
    assert!(!p.trackers[2].calibration.mounting_reset_done);
    let mut c = e.export_config();
    c.reset_mounting_feet = true;
    c.save_mounting_reset = true;
    e.configure(2, c).unwrap();
    e.reset(2, ResetKind::Mounting).unwrap();
    let p = e.tick(2).unwrap();
    assert!(p.trackers[1].calibration.mounting_reset_done);
    assert!(!p.trackers[2].calibration.mounting_reset_done);
    assert!(e
        .export_config()
        .saved_mounting_resets
        .contains_key(&B::LeftFoot));
    e.reset_selected(
        3,
        ResetKind::Mounting,
        &BTreeSet::from([B::LeftIndexProximal]),
    )
    .unwrap();
    assert!(
        e.tick(3).unwrap().trackers[2]
            .calibration
            .mounting_reset_done
    );
}
