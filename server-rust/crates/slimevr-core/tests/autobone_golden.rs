use serde_json::Value;
use slimevr_core::autobone::{self, AutoBoneConfig, MotionFrame};
use slimevr_core::skeleton::SkeletonConfig;
#[test]
fn complete_kotlin_training_matches_epochs_offsets_filtering_and_acceptance() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/autobone-golden.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "83941fd38e91cc91ca6b360deab5c2ae986dd1b6"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let initial: SkeletonConfig = serde_json::from_value(case["initial"].clone()).unwrap();
        let cfg: AutoBoneConfig = serde_json::from_value(case["config"].clone()).unwrap();
        let frames: Vec<MotionFrame> = serde_json::from_value(case["frames"].clone()).unwrap();
        let mut progress = Vec::new();
        let actual = autobone::optimize_with_progress(
            initial,
            &frames,
            case["target_height"].as_f64().map(|v| v as f32),
            cfg,
            |epoch| progress.push(serde_json::to_value(epoch).unwrap()),
        )
        .unwrap();
        assert_eq!(
            progress,
            serde_json::to_value(&actual.epochs)
                .unwrap()
                .as_array()
                .unwrap()
                .clone()
        );
        let expected = &case["expected"];
        let name = case["name"].as_str().unwrap();
        let near = |a: f32, e: &Value, key: &str| {
            let e = e.as_f64().unwrap() as f32;
            assert!(
                (a - e).abs()
                    < if key.contains("offset.") {
                        // Iterative acceptance amplifies f32 FK rounding. Bound lengths to 0.1 mm,
                        // or 1 mm for HMD-only motion where yaw fallback and acceptance ties dominate.
                        if name == "head_only" {
                            1e-3
                        } else {
                            1e-4
                        }
                    } else {
                        (if name == "head_only" { 1e-5 } else { 3e-6 }) * (1.0 + e.abs())
                    },
                "{name}.{key}: actual {a}, expected {e}"
            );
        };
        assert_eq!(
            actual.accepted,
            expected["accepted"].as_bool().unwrap(),
            "{name}.accepted"
        );
        assert_eq!(
            actual.frames_used,
            expected["frames_used"].as_u64().unwrap() as usize,
            "{name}.frames_used"
        );
        near(
            actual.target_height,
            &expected["target_height"],
            "target_height",
        );
        near(
            actual.estimated_height,
            &expected["estimated_height"],
            "height",
        );
        assert_eq!(
            actual.epochs.len(),
            expected["epochs"].as_array().unwrap().len(),
            "{name}.epochs"
        );
        for (epoch, e) in actual
            .epochs
            .iter()
            .zip(expected["epochs"].as_array().unwrap())
        {
            assert_eq!(
                epoch.epoch,
                e["epoch"].as_u64().unwrap() as u32,
                "{name}.epoch"
            );
            assert_eq!(
                epoch.steps,
                e["steps"].as_u64().unwrap() as usize,
                "{name}.steps"
            );
            let offsets = serde_json::to_value(epoch.skeleton).unwrap();
            for (key, v) in e["offsets"].as_object().unwrap() {
                near(
                    offsets[key].as_f64().unwrap() as f32,
                    v,
                    &format!("epoch{}.offset.{}", epoch.epoch, key),
                );
            }
            near(epoch.mean_error, &e["mean_error"], "epoch mean");
            near(
                epoch.standard_deviation,
                &e["standard_deviation"],
                "epoch sd",
            );
        }
        near(
            actual.final_error,
            &expected["epochs"].as_array().unwrap().last().unwrap()["mean_error"],
            "final_error",
        );
        let offsets = serde_json::to_value(actual.skeleton).unwrap();
        for (key, e) in expected["offsets"].as_object().unwrap() {
            near(
                offsets[key].as_f64().unwrap() as f32,
                e,
                &format!("offset.{key}"),
            );
        }
    }
}
