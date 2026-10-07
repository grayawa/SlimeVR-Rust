use serde_json::Value;
use slimevr_core::{
    skeleton::{BoneLinks, BonePose, HeadPose, Skeleton, SkeletonConfig, SkeletonPose},
    Quaternion as Q, Vector3 as V,
};
use slimevr_server::bvh::{Recorder, FRAME_INTERVAL};
use std::{collections::BTreeMap, fs};

fn pose(value: &Value) -> SkeletonPose {
    let mut pose = SkeletonPose::default();
    for (name, b) in value["bones"].as_object().unwrap() {
        pose.bones.insert(
            name.clone(),
            BonePose {
                head: serde_json::from_value(b["head"].clone()).unwrap(),
                tail: serde_json::from_value(b["tail"].clone()).unwrap(),
                rotation: serde_json::from_value(b["rotation"].clone()).unwrap(),
                rotation_offset: serde_json::from_value(b["rotation_offset"].clone()).unwrap(),
                length: b["length"].as_f64().unwrap() as f32,
            },
        );
    }
    for (name, b) in value["hierarchy"].as_object().unwrap() {
        pose.hierarchy.insert(
            name.clone(),
            BoneLinks {
                parent: b["parent"].as_str().map(str::to_owned),
                children: b["children"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().to_owned())
                    .collect(),
            },
        );
    }
    pose
}
fn solved() -> SkeletonPose {
    Skeleton::new(SkeletonConfig::default())
        .solve(
            &BTreeMap::new(),
            Some(HeadPose {
                position: Some(V::new(0.2, 1.7, -0.1)),
                rotation: Q::IDENTITY,
            }),
            false,
        )
        .unwrap()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/bvh-golden.json")).unwrap()
}
fn rows(text: &str) -> Vec<Vec<f32>> {
    text.split_once("Frame Time: 0.01\n")
        .unwrap()
        .1
        .lines()
        .map(|l| {
            l.split_whitespace()
                .map(|v| v.parse::<f32>().unwrap())
                .collect()
        })
        .collect()
}
fn from_channels(v: &[f32]) -> Q {
    Q::rotation_z(v[0].to_radians())
        * Q::rotation_x(v[1].to_radians())
        * Q::rotation_y(v[2].to_radians())
}

#[test]
fn matches_original_kotlin_hierarchy_offsets_root_and_rotations() {
    let golden = fixture();
    assert_eq!(
        golden["reference_commit"],
        "83941fd38e91cc91ca6b360deab5c2ae986dd1b6"
    );
    let dir = tempfile::tempdir().unwrap();
    let mut max_angle = 0.0f32;
    for case in golden["cases"].as_array().unwrap() {
        let poses: Vec<_> = case["poses"].as_array().unwrap().iter().map(pose).collect();
        let mut recorder = Recorder::start(dir.path(), &poses[0]).unwrap();
        for p in &poses {
            recorder.tick(p, FRAME_INTERVAL).unwrap();
        }
        let saved = recorder.finish().unwrap();
        assert_eq!(saved.frames, poses.len() as u64);
        let actual = fs::read_to_string(saved.path).unwrap();
        let expected = case["bvh"].as_str().unwrap();
        let actual_header: Vec<_> = actual
            .split_once("MOTION")
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        let expected_header: Vec<_> = expected
            .split_once("MOTION")
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        assert_eq!(
            actual_header.len(),
            expected_header.len(),
            "{}",
            case["name"]
        );
        for (a, e) in actual_header.iter().zip(expected_header) {
            if let Ok(e) = e.parse::<f32>() {
                assert!(
                    (a.parse::<f32>().unwrap() - e).abs() < 4e-6,
                    "{}: {a} != {e}",
                    case["name"]
                );
            } else {
                assert_eq!(*a, e, "{}", case["name"]);
            }
        }
        let actual_rows = rows(&actual);
        let expected_rows = rows(expected);
        assert_eq!(actual_rows.len(), expected_rows.len());
        for (a, e) in actual_rows.iter().zip(expected_rows) {
            assert_eq!(a.len(), e.len());
            for i in 0..3 {
                assert!((a[i] - e[i]).abs() < 1e-6);
            }
            let (actual_angles, actual_remainder) = a[3..].as_chunks::<3>();
            let (expected_angles, expected_remainder) = e[3..].as_chunks::<3>();
            assert!(actual_remainder.is_empty() && expected_remainder.is_empty());
            for (a, e) in actual_angles.iter().zip(expected_angles) {
                // Equivalent Euler representations are allowed at +/- 90-degree singularities.
                let angle = from_channels(a).angle_to_r(from_channels(e)).to_degrees();
                max_angle = max_angle.max(angle);
                assert!(
                    angle < 0.005,
                    "{}: {a:?} != {e:?}, angle={angle}",
                    case["name"]
                );
            }
        }
        assert!(actual.contains(&format!("Frames: {:<19}\n", poses.len())));
    }
    println!("bvh_reference_max_rotation_error_degrees={max_angle}");
}

#[test]
fn sampling_matches_original_tick_reducer_and_never_bursts() {
    let golden = fixture();
    let dir = tempfile::tempdir().unwrap();
    for case in golden["tick_cases"].as_array().unwrap() {
        let mut pose = solved();
        let mut recorder = Recorder::start(dir.path(), &pose).unwrap();
        for (index, dt) in case["deltas"].as_array().unwrap().iter().enumerate() {
            pose.bones.get_mut("hip").unwrap().tail.x = index as f32;
            recorder.tick(&pose, dt.as_f64().unwrap() as f32).unwrap();
        }
        let saved = recorder.finish().unwrap();
        let fired = case["fired"].as_array().unwrap();
        assert_eq!(saved.frames, fired.len() as u64);
        let text = fs::read_to_string(saved.path).unwrap();
        let rows = rows(&text);
        assert_eq!(rows.len(), fired.len());
        for (row, expected) in rows.iter().zip(fired) {
            assert_eq!(row[0], expected.as_u64().unwrap() as f32);
        }
    }
}

#[test]
fn file_lifecycle_preserves_existing_files_and_finalizes_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let pose = solved();
    let path = dir.path().join("BVH-Recording1.bvh");
    fs::write(&path, "existing recording").unwrap();
    assert!(Recorder::start(&path, &pose).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "existing recording");
    assert!(Recorder::start(&dir.path().join("wrong.txt"), &pose).is_err());
    assert!(Recorder::start(dir.path(), &SkeletonPose::default()).is_err());
    {
        let mut recorder = Recorder::start(dir.path(), &pose).unwrap();
        recorder.tick(&pose, FRAME_INTERVAL).unwrap();
        recorder.tick(&pose, FRAME_INTERVAL).unwrap();
    }
    let text = fs::read_to_string(dir.path().join("BVH-Recording2.bvh")).unwrap();
    assert!(text.contains("Frames: 2                  \n"));
    assert_eq!(rows(&text).len(), 2);
    let mut empty = Recorder::start(dir.path(), &pose).unwrap();
    assert_eq!(empty.finish().unwrap().frames, 0);
    assert_eq!(empty.finish().unwrap().frames, 0);
}

#[test]
fn rig_changes_stop_before_writing_inconsistent_frames() {
    let dir = tempfile::tempdir().unwrap();
    let mut pose = solved();
    let mut recorder = Recorder::start(dir.path(), &pose).unwrap();
    recorder.tick(&pose, FRAME_INTERVAL).unwrap();
    pose.bones.get_mut("left_upper_leg").unwrap().length += 0.01;
    assert!(recorder
        .tick(&pose, FRAME_INTERVAL)
        .unwrap_err()
        .to_string()
        .contains("bone lengths or hierarchy changed"));
    let saved = recorder.finish().unwrap();
    assert_eq!(saved.frames, 1);
    assert_eq!(rows(&fs::read_to_string(saved.path).unwrap()).len(), 1);
    let mut pose = solved();
    let mut recorder = Recorder::start(dir.path(), &pose).unwrap();
    pose.hierarchy
        .get_mut("left_upper_arm")
        .unwrap()
        .children
        .clear();
    assert!(recorder.tick(&pose, FRAME_INTERVAL).is_err());
    assert_eq!(recorder.finish().unwrap().frames, 0);
}
