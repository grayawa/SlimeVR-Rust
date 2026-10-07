use slimevr_server::pose_recording::{self, Recording};
#[test]
fn original_kotlin_pfs_and_pfr_bytes_match_both_directions() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pose-recording-golden.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let recording: Recording = serde_json::from_value(case.clone()).unwrap();
        for (kind, pfr) in [("pfs", false), ("pfr", true)] {
            let bytes =
                slimevr_server::recording::decode_hex(case["expected"][kind].as_str().unwrap())
                    .unwrap();
            assert_eq!(recording.encode(pfr).unwrap(), bytes);
            assert_eq!(pose_recording::decode(&bytes, pfr).unwrap(), recording);
        }
    }
}
#[test]
fn imported_motion_preserves_interval_player_state_and_body_mapping() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../slimevr-core/tests/fixtures/autobone-golden.json"
    ))
    .unwrap();
    let frames: Vec<slimevr_core::autobone::MotionFrame> =
        serde_json::from_value(fixture["cases"][0]["frames"].clone()).unwrap();
    let recording = Recording::from_motion(&frames, 0.02);
    for pfr in [false, true] {
        let loaded = pose_recording::decode(&recording.encode(pfr).unwrap(), pfr)
            .unwrap()
            .motion_frames()
            .unwrap();
        assert_eq!(
            serde_json::to_value(loaded).unwrap(),
            serde_json::to_value(&frames).unwrap()
        );
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.pfr"), recording.encode(true).unwrap()).unwrap();
    std::fs::write(dir.path().join("a.pfs"), recording.encode(false).unwrap()).unwrap();
    let imported = pose_recording::load_directory(dir.path()).unwrap();
    assert_eq!(imported.len(), 2);
    assert_eq!(imported[0].0.file_name().unwrap(), "a.pfs");
    for (_, motion) in imported {
        assert_eq!(
            serde_json::to_value(motion).unwrap(),
            serde_json::to_value(&frames).unwrap()
        );
    }
    let mut recording = recording;
    recording
        .trackers
        .iter_mut()
        .find(|t| t.frames[0].body == Some(slimevr_core::skeleton::BodyPosition::Head))
        .unwrap()
        .frames[1] = Default::default();
    let motion = recording.motion_frames().unwrap();
    assert_eq!(
        serde_json::to_value(motion[1].head).unwrap(),
        serde_json::to_value(motion[0].head).unwrap()
    );
}
#[test]
fn saving_preserves_numbered_files_and_atomically_replaces_last_recording() {
    let dir = tempfile::tempdir().unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pose-recording-golden.json")).unwrap();
    let recording: Recording = serde_json::from_value(fixture["cases"][0].clone()).unwrap();
    let first = pose_recording::save_next(dir.path(), &recording).unwrap();
    let bytes = std::fs::read(&first).unwrap();
    let second = pose_recording::save_next(dir.path(), &recording).unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read(&first).unwrap(), bytes);
    let last = pose_recording::save_last(dir.path(), &recording).unwrap();
    assert_eq!(pose_recording::load(&last).unwrap(), recording);
    let mut next = recording;
    next.interval = 0.01;
    pose_recording::save_last(dir.path(), &next).unwrap();
    assert_eq!(pose_recording::load(&last).unwrap(), next);
}
#[test]
fn legacy_designation_frames_and_malformed_input_are_handled_explicitly() {
    // PFR: one tracker, one frame, historical designation string + quaternion + position.
    let mut bytes = Vec::new();
    bytes.extend(1u32.to_be_bytes());
    bytes.extend([0, 1, b'H']);
    bytes.extend(1u32.to_be_bytes());
    bytes.extend(7u32.to_be_bytes());
    bytes.extend([0, 9]);
    bytes.extend(b"body:head");
    for value in [0f32, 0., 0., 1., 0., 1.7, 0.] {
        bytes.extend(value.to_be_bytes());
    }
    let r = pose_recording::decode(&bytes, true).unwrap();
    assert_eq!(
        r.trackers[0].frames[0].body,
        Some(slimevr_core::skeleton::BodyPosition::Head)
    );
    for n in 0..bytes.len() {
        assert!(pose_recording::decode(&bytes[..n], true).is_err());
    }
    assert!(pose_recording::decode(&[255], false).is_err());
    assert!(pose_recording::decode(&[2, 0, 255, 255, 255, 255], false).is_err());
    assert!(pose_recording::decode(&[0, 0x7f, 0x80, 0, 0], false).is_err());
}
