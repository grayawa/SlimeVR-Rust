use serde_yaml_ng::Value;
use slimevr_core::{
    calibration::ArmsResetMode, filtering::FilterType, skeleton::BodyPosition as B, Quaternion as Q,
};
use slimevr_server::{api::FrontendConfig, config};
use std::{fs, process::Command};

const ORIGINAL: &str = include_str!("fixtures/vrconfig-v15.yml");
fn load(text: &str) -> FrontendConfig {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yml");
    fs::write(&path, text).unwrap();
    FrontendConfig::load(&path).unwrap()
}

#[test]
fn native_velocity_setting_defaults_validates_and_roundtrips_without_losing_extensions() {
    assert!(!load("version: '15'\n").pose.send_derived_velocity);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yml");
    let original =
        "version: '15'\nvelocityConfig:\n  sendDerivedVelocity: true\n  futureOption: keep\n";
    fs::write(&path, original).unwrap();
    let mut config = FrontendConfig::load(&path).unwrap();
    assert!(config.pose.send_derived_velocity);
    config.pose.send_derived_velocity = false;
    config.save(Some(&path)).unwrap();
    let reloaded = FrontendConfig::load(&path).unwrap();
    assert!(!reloaded.pose.send_derived_velocity);
    assert_eq!(
        reloaded.yaml["velocityConfig"]["futureOption"].as_str(),
        Some("keep")
    );
    assert_eq!(
        fs::read_to_string(path.with_extension("yml.bak")).unwrap(),
        original
    );
    assert!(config::from_yaml(
        serde_yaml_ng::from_str("velocityConfig: {sendDerivedVelocity: invalid}").unwrap()
    )
    .is_err());
}

#[test]
fn original_fields_drive_runtime_settings_and_udp_sensor_bindings() {
    let c = load(ORIGINAL);
    assert_eq!(c.tracker_port, 7001);
    assert_eq!(c.allowed_macs.len(), 2);
    assert_eq!(c.pose.bindings.len(), 2);
    assert_eq!(c.pose.bindings[1].sensor_id, 1);
    assert_eq!(c.pose.bindings[1].body, B::LeftLowerLeg);
    assert_eq!(c.pose.bindings[1].mounting, Q::new(0.383, 0.0, 0.924, 0.0));
    assert_eq!(c.tracker_names["02:00:00:00:00:01/0"], "Chest sensor");
    assert_eq!(c.pose.filter.mode, FilterType::Smoothing);
    assert_eq!(c.pose.filter.amount, 0.35);
    assert_eq!(c.pose.skeleton.hips_width, 0.31);
    assert!(!c.pose.skeleton.extended_spine);
    assert!(!c.pose.skeleton.force_arms_from_hmd);
    assert!(!c.pose.legs.floor_clip);
    assert!(c.pose.legs.toe_snap && c.pose.legs.always_use_floor_clip);
    assert!(c.pose.localizer.enabled);
    assert!((c.pose.hmd_height.unwrap() - 1.7).abs() < 1e-6);
    assert_eq!(c.pose.arms_reset_mode, ArmsResetMode::TposeUp);
    assert_eq!(c.pose.full_reset_delay_ms, 2250);
    assert_eq!(c.pose.mounting_reset_delay_ms, 1500);
    assert!(c.pose.save_mounting_reset);
    assert!(c.pose.saved_mounting_resets.contains_key(&B::Chest));
    assert!(c.pose.taps.enabled && !c.pose.taps.full_enabled);
    assert_eq!(c.pose.taps.yaw_tracker, Some(B::Chest));
    assert_eq!(c.pose.taps.yaw_taps, 4);
    assert_eq!(c.pose.taps.yaw_delay_ms, 450);
    assert_eq!(c.pose.alignment.standing.upper_leg_degrees, 12.0);
    assert_eq!(c.auto_bone.epochs, 12);
    assert_eq!(c.auto_bone.seed, (-17i64) as u64);
    assert!(c.auto_bone.filter_outliers);
    assert_eq!(c.sample_count, 1200);
}

#[test]
fn save_updates_original_keys_preserves_unimplemented_settings_and_keeps_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yaml");
    fs::write(&path, ORIGINAL).unwrap();
    let mut c = FrontendConfig::load(&path).unwrap();
    let before = c.yaml.clone();
    c.pose.filter.amount = 0.57;
    c.pose.skeleton.hips_width = 0.36;
    c.pose.bindings[0].body = B::Hip;
    c.tracker_names
        .insert("02:00:00:00:00:01/0".into(), "Pelvis".into());
    c.save(Some(&path)).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        fs::read_to_string(path.with_extension("yaml.bak")).unwrap(),
        ORIGINAL
    );
    assert!(!dir.path().join("rust-backend.json").exists());
    let saved: Value = serde_yaml_ng::from_str(&text).unwrap();
    assert_eq!(saved["version"], Value::String("15".into()));
    assert_eq!(
        saved["trackers"]["udp://02:00:00:00:00:01/0"]["designation"].as_str(),
        Some("body:hip")
    );
    for key in ["hidConfig", "keybindings", "trackingChecklist", "custom"] {
        assert_eq!(saved[key], before[key], "{key}");
    }
    for section in ["oscRouter", "vrcOSC", "vmc"] {
        for (key, value) in before[section].as_mapping().unwrap() {
            assert_eq!(
                &saved[section][key.as_str().unwrap()],
                value,
                "{section}.{key:?}"
            );
        }
    }
    assert_eq!(FrontendConfig::load(&path).unwrap().osc, c.osc);
    assert_eq!(
        saved["bridges"]["steamvr"]["trackers"]["feet"],
        before["bridges"]["steamvr"]["trackers"]["feet"]
    );
    assert!(FrontendConfig::load(&path)
        .unwrap()
        .steam_vr
        .enabled("waist"));
    for key in [
        "hide",
        "allowDriftCompensation",
        "shouldHaveMagEnabled",
        "customProperty",
    ] {
        assert_eq!(
            saved["trackers"]["udp://02:00:00:00:00:01/0"][key],
            before["trackers"]["udp://02:00:00:00:00:01/0"][key]
        );
    }
    assert_eq!(saved["trackers"]["HMD"], before["trackers"]["HMD"]);
    assert_eq!(
        saved["trackers"]["hid://device/0"],
        before["trackers"]["hid://device/0"]
    );
    assert_eq!(
        saved["resetsConfig"]["resetHmdPitch"],
        before["resetsConfig"]["resetHmdPitch"]
    );
    assert_eq!(
        saved["skeleton"]["toggles"]["correctConstraints"],
        before["skeleton"]["toggles"]["correctConstraints"]
    );
    assert_eq!(
        saved["skeleton"]["offsets"]["customOffset"],
        before["skeleton"]["offsets"]["customOffset"]
    );
    let reloaded = FrontendConfig::load(&path).unwrap();
    assert_eq!(reloaded.pose.filter.amount, 0.57);
    assert_eq!(reloaded.pose.skeleton.hips_width, 0.36);
    assert_eq!(reloaded.pose.bindings[0].body, B::Hip);
    assert!((reloaded.pose.hmd_height.unwrap() - c.pose.hmd_height.unwrap()).abs() < 1e-6);
    assert_eq!(reloaded.yaml["custom"], c.yaml["custom"]);
}

#[test]
fn forget_and_unassign_are_persisted_without_deleting_other_tracker_sources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yml");
    let mut c = load(ORIGINAL);
    c.pose.bindings.remove(0);
    c.save(Some(&path)).unwrap();
    assert_eq!(FrontendConfig::load(&path).unwrap().pose.bindings.len(), 1);
    let mac = "02:00:00:00:00:02";
    c.allowed_macs.retain(|m| m != mac);
    c.pose.bindings.retain(|b| b.device_key != mac);
    c.tracker_names.retain(|k, _| !k.starts_with(mac));
    config::forget_device(&mut c, mac);
    c.save(Some(&path)).unwrap();
    let c = FrontendConfig::load(&path).unwrap();
    assert!(c.pose.bindings.is_empty());
    assert!(c.yaml["trackers"]["udp://02:00:00:00:00:02/1"].is_null());
    assert!(c.yaml["trackers"]["HMD"].is_mapping());
}

#[test]
fn legacy_v1_and_v12_migrations_match_upstream_renames_splits_and_defaults() {
    let c = load("version: '1'\nbody: {torsoLength: 0.6, chestDistance: 0.3, waistDistance: 0.05, legsLength: 1.0, kneeHeight: 0.55, shoulersWidth: 0.4, chestOffset: 0.02, controllerDistanceY: 0.03}\nfilters: {type: smoothing, amount: 3.0}\ntrackers:\n- name: udp://02:00:00:00:00:01/0\n  designation: body:chest\n  mountingRotation: {w: 1, x: 0, y: 0, z: 0}\n  customName: Old sensor\nautoBone: {sampleCount: 1000}\nbridge: {steamvr: {enabled: true}}\ntapDetection: {quickResetTaps: 5, resetEnabled: false}\n");
    assert!((c.pose.skeleton.chest_length - 0.15).abs() < 1e-6);
    assert!((c.pose.skeleton.upper_chest_length - 0.15).abs() < 1e-6);
    assert!((c.pose.skeleton.waist_length - 0.25).abs() < 1e-6);
    assert!((c.pose.skeleton.upper_leg_length - 0.45).abs() < 1e-6);
    assert_eq!(c.pose.skeleton.shoulders_width, 0.4);
    assert_eq!(c.pose.skeleton.chest_offset, -0.02);
    assert_eq!(c.pose.skeleton.hand_y, 0.03);
    assert_eq!(c.pose.filter.amount, 0.2);
    assert_eq!(c.pose.bindings[0].mounting, Q::IDENTITY);
    assert_eq!(c.pose.taps.yaw_taps, 5);
    assert!(!c.pose.taps.full_enabled);
    assert_eq!(c.sample_count, 1500);
    assert_eq!(c.allowed_macs, ["02:00:00:00:00:01"]);
    assert!(c.yaml["bridges"]["steamvr"]["enabled"].as_bool().unwrap());
    let c = load("version: '12'\nautoBone: {targetHmdHeight: 1.7, numEpochs: 100, slideErrorFactor: 0.0, offsetSlideErrorFactor: 1.0, bodyProportionErrorFactor: 0.25}\ntrackers:\n  udp://02:00:00:00:00:01/0: {designation: 'body:hip'}\ntrackingChecklist: {ignoredStepsIds: [a, b]}\n");
    assert_eq!(c.auto_bone.epochs, 50);
    assert_eq!(c.auto_bone.slide_factor, 1.0);
    assert_eq!(c.auto_bone.offset_slide_factor, 0.0);
    assert_eq!(c.auto_bone.proportion_factor, 0.05);
    assert_eq!(c.pose.hmd_height, Some(1.7));
    assert!(c.yaml["trackingChecklist"]["ignoredStepsIds"]
        .as_sequence()
        .unwrap()
        .is_empty());
}

#[test]
fn legacy_json_and_rust_only_metadata_roundtrip_through_one_yaml_file() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("rust-backend.json");
    let yaml = dir.path().join("vrconfig.yml");
    let mut original = FrontendConfig::default();
    original.pose.legs.enabled = false;
    original.pose.imu_types.insert(B::Chest, 13);
    original.pose.flex_resistance.insert(B::LeftIndexProximal);
    original.device_ids.insert("02:00:00:00:00:01".into(), 12);
    fs::write(&old, serde_json::to_string(&original).unwrap()).unwrap();
    let loaded = FrontendConfig::load(&old).unwrap();
    loaded.save(Some(&yaml)).unwrap();
    let c = FrontendConfig::load(&yaml).unwrap();
    assert_eq!(
        serde_json::to_value(&original).unwrap(),
        serde_json::to_value(c).unwrap()
    );
    assert!(yaml.exists() && old.exists());
    let first = load("version: '15'\ntrackers:\n  udp://aa:bb:cc:dd:ee:01/0: {designation: 'body:chest'}\nknownDevices: ['aa:bb:cc:dd:ee:01']\n");
    let yaml = config::to_yaml(&first).unwrap();
    let second = config::from_yaml(yaml).unwrap();
    assert_eq!(
        second.pose.bindings.len(),
        1,
        "case aliases must not create duplicate sensors"
    );
}

#[test]
fn invalid_and_future_configurations_fail_without_modifying_original_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yml");
    for text in [
        "version: '16'\n",
        "version: nonsense\n",
        "skeleton: {offsets: {hipsWidth: -1}}\n",
        "skeleton: {offsets: {hipsWidth: .nan}}\n",
        "trackers: []\n",
        "filters: {type: unsupported}\n",
        "filters: [0, 1]\n",
        "resetsConfig: {fullResetDelay: -1}\n",
        "knownDevices: [not-a-mac]\n",
        "version: '15'\nversion: '15'\n",
    ] {
        fs::write(&path, text).unwrap();
        assert!(
            FrontendConfig::load(&path).is_err(),
            "accepted invalid configuration {text}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
    fs::write(&path, ORIGINAL).unwrap();
    let mut c = FrontendConfig::load(&path).unwrap();
    c.pose.skeleton.hips_width = -1.0;
    assert!(c.save(Some(&path)).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
    assert!(!path.with_extension("yml.bak").exists());
}

#[test]
fn real_cli_uses_original_default_file_and_udp_port_and_migrates_legacy_state_once() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("config")).unwrap();
    let path = dir.path().join("vrconfig.yml");
    fs::write(
        &path,
        "version: '15'\nserver: {trackerPort: 0}\ncustom: keep me\n",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .current_dir(dir.path())
        .args([
            "listen",
            "--no-steamvr",
            "--api-bind",
            "127.0.0.1:0",
            "--no-discovery",
            "--run-for",
            "1",
            "--pose-output-ms",
            "1000",
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let ready: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&run.stdout).lines().next().unwrap()).unwrap();
    assert_eq!(ready["type"], "listening");
    assert!(!ready["bind"].as_str().unwrap().ends_with(":6969"));
    let diagnostics: Vec<serde_json::Value> = String::from_utf8_lossy(&run.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&run.stderr).lines())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let save = diagnostics
        .iter()
        .find(|entry| entry["type"] == "config_save_timing")
        .unwrap();
    assert_eq!(save["outcome"], "saved");
    assert_eq!(save["files_written"], 2); // Existing YAML and its backup.
    assert_eq!(save["sync_all_calls"], 2);
    assert!(save["file_io_ms"].as_f64().unwrap() >= save["sync_all_ms"].as_f64().unwrap());
    let timing = diagnostics
        .iter()
        .find(|entry| entry["type"] == "runtime_timing")
        .unwrap();
    assert!(timing["api_live_snapshots"].as_u64().unwrap() > 0);
    assert!(timing["api_live_snapshot_ms"]["max"].as_f64().unwrap() >= 0.);
    assert_eq!(timing["steamvr_output_batches_written"], 0);
    assert_eq!(timing["steamvr_output_queue_full"], 0);
    let c = FrontendConfig::load(&path).unwrap();
    assert_eq!(c.yaml["custom"].as_str(), Some("keep me"));
    assert!(!dir.path().join("rust-backend.json").exists());
    let legacy = tempfile::tempdir().unwrap();
    let old = legacy.path().join("rust-backend.json");
    fs::write(
        &old,
        "{\"version\":1,\"pose\":{\"filter\":{\"mode\":\"smoothing\",\"amount\":0.4}}}",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .args([
            "listen",
            "--bind",
            "127.0.0.1:0",
            "--no-steamvr",
            "--api-bind",
            "127.0.0.1:0",
            "--config",
        ])
        .arg(legacy.path().join("vrconfig.yml"))
        .args(["--no-discovery", "--run-for", "1"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        FrontendConfig::load(&legacy.path().join("vrconfig.yml"))
            .unwrap()
            .pose
            .filter
            .amount,
        0.4
    );
    assert_eq!(
        fs::read_to_string(old).unwrap(),
        "{\"version\":1,\"pose\":{\"filter\":{\"mode\":\"smoothing\",\"amount\":0.4}}}"
    );
}

#[test]
fn extended_calibration_and_training_flags_use_native_yaml_and_preserve_extensions() {
    let mut c = load("resetsConfig: {resetHmdPitch: true, resetMountingFeet: true, futureReset: keep}\nautoBone: {calcInitError: true, useSkeletonHeight: true, futureTraining: keep}\n");
    assert!(c.pose.reset_hmd_pitch && c.pose.reset_mounting_feet);
    assert!(c.auto_bone.calc_initial_error && c.auto_bone.use_skeleton_height);
    c.pose.reset_hmd_pitch = false;
    c.pose.reset_mounting_feet = false;
    c.auto_bone.calc_initial_error = false;
    c.auto_bone.use_skeleton_height = false;
    let c = config::from_yaml(config::to_yaml(&c).unwrap()).unwrap();
    assert!(!c.pose.reset_hmd_pitch && !c.pose.reset_mounting_feet);
    assert!(!c.auto_bone.calc_initial_error && !c.auto_bone.use_skeleton_height);
    assert_eq!(c.yaml["resetsConfig"]["futureReset"].as_str(), Some("keep"));
    assert_eq!(c.yaml["autoBone"]["futureTraining"].as_str(), Some("keep"));
}
