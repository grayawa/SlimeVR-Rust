use serde_json::{json, Value};
use slimevr_core::{
    autobone::skeleton_height,
    filtering::{FilterConfig, FilterType},
    pose::{PoseConfig, TrackerBinding},
    skeleton::{BodyPosition as B, HeadPose, Skeleton, SkeletonConfig},
    Quaternion as Q, TrackerSample, Vector3 as V,
};
use std::{collections::BTreeMap, process::Command};

#[test]
fn autobone_cli_fits_motion_writes_reloadable_config_and_preserves_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("motion.jsonl");
    let config = dir.path().join("pose.json");
    let settings = dir.path().join("autobone.json");
    let output = dir.path().join("fitted.json");
    let bodies = [
        B::Chest,
        B::Hip,
        B::LeftUpperLeg,
        B::RightUpperLeg,
        B::LeftLowerLeg,
        B::RightLowerLeg,
    ];
    let truth = SkeletonConfig {
        enforce_constraints: false,
        extended_knee: false,
        upper_leg_length: 0.46,
        lower_leg_length: 0.54,
        waist_length: 0.24,
        ..Default::default()
    };
    let height = skeleton_height(truth);
    let c = PoseConfig {
        bindings: bodies
            .iter()
            .enumerate()
            .map(|(i, body)| TrackerBinding {
                device_key: format!("device{i}"),
                sensor_id: 0,
                body: *body,
                mounting: Q::IDENTITY,
            })
            .collect(),
        skeleton: SkeletonConfig {
            enforce_constraints: false,
            extended_knee: false,
            ..Default::default()
        },
        filter: FilterConfig {
            mode: FilterType::None,
            amount: 0.0,
        },
        ..Default::default()
    };
    std::fs::write(&config, serde_json::to_vec(&c).unwrap()).unwrap();
    std::fs::write(
        &settings,
        br#"{"epochs":12,"proportion_factor":0,"max_final_error":0.5}"#,
    )
    .unwrap();
    let mut lines = Vec::new();
    for i in 0..48 {
        let t = i as f32 * 0.17;
        let mut rotations: BTreeMap<_, _> = bodies.iter().map(|b| (*b, Q::IDENTITY)).collect();
        rotations.insert(B::Chest, Q::rotation_x(t.sin() * 0.25));
        for b in [B::LeftUpperLeg, B::RightUpperLeg] {
            rotations.insert(b, Q::rotation_x(-0.6 * t.sin().abs()));
        }
        for b in [B::LeftLowerLeg, B::RightLowerLeg] {
            rotations.insert(b, Q::rotation_x(0.5 * t.sin().abs()));
        }
        let p = Skeleton::new(truth)
            .solve(
                &rotations,
                Some(HeadPose {
                    rotation: Q::IDENTITY,
                    position: Some(V::ZERO),
                }),
                false,
            )
            .unwrap();
        let ankle = p.bones["left_lower_leg"].tail;
        let at = i * 20;
        lines.push(json!({"type":"head","at_ms":at,"rotation":Q::IDENTITY,"position":V::new(0.0,-ankle.y,-ankle.z)}));
        for binding in &c.bindings {
            let sample = TrackerSample {
                source: "synthetic".into(),
                device_key: binding.device_key.clone(),
                sensor_id: 0,
                session: 1,
                packet_sequence: i as i64,
                received_at_ms: at,
                sensor_timestamp_us: None,
                packet_rotation: None,
                server_rotation: Some(rotations[&binding.body]),
                packet_acceleration: None,
                server_acceleration: None,
                position: None,
                compatibility_fallback: false,
            };
            lines.push(json!({"type":"sample","sample":sample}));
        }
        lines.push(json!({"type":"tick","at_ms":at}));
    }
    let text = lines
        .into_iter()
        .map(|v| v.to_string() + "\n")
        .collect::<String>();
    std::fs::write(&scene, text).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .arg("autobone")
            .arg(&scene)
            .arg("--config")
            .arg(&config)
            .arg("--settings")
            .arg(&settings)
            .arg("--target-height")
            .arg(height.to_string())
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    let fit = run();
    assert!(
        fit.status.success(),
        "{}",
        String::from_utf8_lossy(&fit.stderr)
    );
    let result: Value = serde_json::from_slice(&fit.stdout).unwrap();
    assert_eq!(result["result"]["accepted"], true);
    assert!(
        result["result"]["evaluation_error"].as_f64().unwrap()
            < result["result"]["initial_error"].as_f64().unwrap()
    );
    let bytes = std::fs::read(&output).unwrap();
    let fitted: PoseConfig = serde_json::from_slice(&bytes).unwrap();
    fitted.validate().unwrap();
    assert!((skeleton_height(fitted.skeleton) - height).abs() < 1e-5);
    assert_eq!(fitted.bindings.len(), 6);
    assert!(
        !run().status.success(),
        "existing configuration must not be overwritten"
    );
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    let yaml_input = dir.path().join("vrconfig.yml");
    let yaml_output = dir.path().join("fitted.yml");
    let frontend = slimevr_server::api::FrontendConfig {
        pose: c.clone(),
        osc: {
            let mut osc = slimevr_server::osc::Settings::default();
            osc.router.enabled = true;
            osc
        },
        auto_bone: serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap(),
        yaml: serde_yaml_ng::from_str(
            "version: '15'\noscRouter:\n  enabled: true\ncustomExtension: keep\n",
        )
        .unwrap(),
        ..Default::default()
    };
    frontend.save(Some(&yaml_input)).unwrap();
    let original_yaml = std::fs::read(&yaml_input).unwrap();
    let yaml_fit = Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .arg("autobone")
        .arg(&scene)
        .arg("--config")
        .arg(&yaml_input)
        .arg("--target-height")
        .arg(height.to_string())
        .arg("--output")
        .arg(&yaml_output)
        .output()
        .unwrap();
    assert!(
        yaml_fit.status.success(),
        "{}",
        String::from_utf8_lossy(&yaml_fit.stderr)
    );
    let yaml_fitted = slimevr_server::api::FrontendConfig::load(&yaml_output).unwrap();
    assert!((skeleton_height(yaml_fitted.pose.skeleton) - height).abs() < 1e-5);
    assert_eq!(yaml_fitted.pose.bindings.len(), 6);
    assert_eq!(
        yaml_fitted.yaml["oscRouter"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(yaml_fitted.yaml["customExtension"].as_str(), Some("keep"));
    assert_eq!(std::fs::read(&yaml_input).unwrap(), original_yaml);
    std::fs::remove_file(&output).unwrap();
    std::fs::write(
        &settings,
        br#"{"epochs":1,"proportion_factor":0,"max_final_error":0}"#,
    )
    .unwrap();
    assert!(!run().status.success());
    assert!(
        !output.exists(),
        "rejected calibration must not write a configuration"
    );
}
