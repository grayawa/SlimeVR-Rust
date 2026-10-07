use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn six_point_scene_runs_resets_pause_head_motion_and_repeats_identically() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .arg("solve")
            .arg(root.join("examples/six-point.scene.jsonl"))
            .arg("--config")
            .arg(root.join("examples/six-point.json"))
            .arg("--frames")
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let frames: Vec<Value> = String::from_utf8(first.stdout.clone())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert!(frames.len() >= 20);
    assert_eq!(frames.last().unwrap()["reset_count"], 2);
    assert!(frames.iter().any(|f| f["paused"] == true));
    assert_eq!(frames[0]["trackers"].as_array().unwrap().len(), 6);
    assert_eq!(frames[0]["skeleton"]["world_anchor_present"], true);
    // Default FK gives 0.12m; LegTweaks initializes the seed with its 2.5mm floor offset.
    assert!(
        (frames[0]["skeleton"]["computed"]["left_foot"]["position"]["y"]
            .as_f64()
            .unwrap()
            - 0.1225)
            .abs()
            < 1e-5
    );
    assert_eq!(first.stdout, run().stdout);
}
