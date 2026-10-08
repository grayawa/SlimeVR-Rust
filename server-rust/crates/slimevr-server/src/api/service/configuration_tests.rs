use super::*;

fn service() -> (Service, PoseEngine, Receiver) {
    let mut config = FrontendConfig::default();
    config.pose.bindings.push(TrackerBinding {
        device_key: "test".into(),
        sensor_id: 0,
        body: B::Chest,
        mounting: slimevr_core::Quaternion::IDENTITY,
    });
    let engine = PoseEngine::new(config.pose.clone()).unwrap();
    let (commands, _) = mpsc::channel(32);
    let (events, _) = broadcast::channel(64);
    (
        Service::new(config, None, commands, events),
        engine,
        Receiver::new(Default::default()).unwrap(),
    )
}

#[test]
fn live_keybind_and_osc_changes_notify_once_and_failed_commit_keeps_current_settings() {
    let (mut service, mut engine, receiver) = service();
    assert!(service.take_hotkeys_update().unwrap().is_none());
    assert!(service.take_osc_update().is_none());
    let mut config = service.config.clone();
    let mut keys = crate::hotkeys::Settings::default();
    keys.bindings[0].value = "CTRL+F11".into();
    keys.bindings[0].delay_ms = 500;
    keys.write(&mut config.yaml).unwrap();
    config.osc.router.port_in += 1;
    let osc = config.osc.clone();
    service.commit(config, &mut engine, &receiver, 1).unwrap();
    assert_eq!(service.take_hotkeys_update().unwrap(), Some(keys.clone()));
    assert_eq!(service.take_osc_update(), Some(osc.clone()));
    service
        .commit(service.config.clone(), &mut engine, &receiver, 2)
        .unwrap();
    assert!(service.take_hotkeys_update().unwrap().is_none());
    assert!(service.take_osc_update().is_none());
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    service.state_path = Some(blocked.join("config.yml"));
    let mut invalid = service.config.clone();
    let mut new_keys = keys.clone();
    new_keys.bindings[0].value = "ALT+F12".into();
    new_keys.write(&mut invalid.yaml).unwrap();
    assert!(service.commit(invalid, &mut engine, &receiver, 3).is_err());
    assert_eq!(
        crate::hotkeys::Settings::read(&service.config.yaml).unwrap(),
        keys
    );
    assert_eq!(service.config.osc, osc);
    assert!(service.take_hotkeys_update().unwrap().is_none());
}

#[test]
fn tick_and_read_requests_observe_metadata_height_and_cleared_mounting_after_revision_changes() {
    let (mut service, mut engine, receiver) = service();
    engine
        .ingest(&InputEvent {
            at_ms: 1,
            kind: EventKind::SensorMetadata {
                device_key: "test".into(),
                sensor_id: 0,
                imu_type: 13,
                data_type: 0,
                magnetometer_enabled: true,
            },
        })
        .unwrap();
    engine.tick(1).unwrap();
    service.after_tick(&engine, &receiver, 1);
    assert_eq!(service.config.pose.imu_types[&B::Chest], 13);
    assert!(service.config.pose.magnetometers.contains(&B::Chest));
    engine.apply_height(2, 1.7).unwrap();
    // An API read before the next tick must already see the new configuration.
    service.sync_pose_config(&engine);
    assert_eq!(service.config.pose.hmd_height, Some(1.7));
    let mut config = engine.export_config();
    config
        .saved_mounting_resets
        .insert(B::Chest, slimevr_core::Quaternion::rotation_y(0.2));
    engine.configure(3, config).unwrap();
    service.sync_pose_config(&engine);
    assert!(!service.config.pose.saved_mounting_resets.is_empty());
    engine.clear_mounting(4).unwrap();
    engine.tick(4).unwrap();
    service.after_tick(&engine, &receiver, 4);
    assert!(service.config.pose.saved_mounting_resets.is_empty());
    for at in 5..=100 {
        engine.tick(at).unwrap();
        service.after_tick(&engine, &receiver, at);
    }
    assert_eq!(service.config.pose.hmd_height, Some(1.7));
    assert_eq!(service.config.pose.imu_types[&B::Chest], 13);
}
