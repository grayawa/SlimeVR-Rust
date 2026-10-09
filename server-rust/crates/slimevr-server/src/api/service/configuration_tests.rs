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

#[tokio::test]
async fn configuration_changes_notify_once_and_disk_failure_keeps_live_settings() {
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
    let mut events = service.events.subscribe();
    service.commit(invalid, &mut engine, &receiver, 3).unwrap();
    assert!(service.finish_config_save().await.is_err());
    assert!(service.poll_config_save().unwrap().result.is_err());
    let mut errors = Vec::new();
    while let Ok(wire) = events.try_recv() {
        if let Wire::Text(message) = wire {
            errors.push(serde_json::from_str::<serde_json::Value>(&message).unwrap());
        }
    }
    assert!(errors.iter().any(|error| error["type"] == "backend_error"));
    assert_eq!(
        crate::hotkeys::Settings::read(&service.config.yaml).unwrap(),
        new_keys.clone()
    );
    assert_eq!(service.config.osc, osc);
    assert_eq!(service.take_hotkeys_update().unwrap(), Some(new_keys));
    let mut invalid = service.config.clone();
    invalid.pose.skeleton.hips_width = -1.;
    assert!(service.commit(invalid, &mut engine, &receiver, 4).is_err());
    assert!(service.config.pose.skeleton.hips_width > 0.);
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

#[test]
fn live_snapshots_share_static_data_and_pose_without_mutating_previous_frames() {
    let (mut service, mut engine, _) = service();
    let mut receiver = Receiver::new(crate::receiver::ReceiverConfig {
        accept_new_devices: true,
        ..Default::default()
    })
    .unwrap();
    let mut handshake: Vec<u8> = [9u32, 13, 1, 0, 0, 0, 22]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect();
    handshake.extend(b"\x05test\0");
    handshake.extend([2, 0, 0, 0, 0, 1]);
    let from = "127.0.0.1:6969".parse().unwrap();
    receiver.receive(from, &crate::protocol::header(3, &handshake), 0);
    receiver.receive(from, &crate::protocol::header(15, &[0, 1, 13]), 0);
    engine.tick(0).unwrap();
    let first = service.live(&receiver, &engine, 0);
    let second = service.live(&receiver, &engine, 1);
    assert_eq!(first.devices.len(), 1);
    assert!(Arc::ptr_eq(&first.config, &second.config));
    assert!(Arc::ptr_eq(&first.pose, &second.pose));
    assert!(Arc::ptr_eq(
        &first.devices[0].metadata,
        &second.devices[0].metadata
    ));
    service.config.yaml["custom"] = serde_yaml_ng::Value::String("changed".into());
    receiver
        .devices
        .get_mut("02:00:00:00:00:01")
        .unwrap()
        .display_name = Some("New name".into());
    let mut rotation = vec![0, 1];
    for v in [0f32, 0., 0., 1.] {
        rotation.extend(v.to_be_bytes());
    }
    rotation.push(3);
    receiver.receive(from, &crate::protocol::header(17, &rotation), 2);
    engine.tick(2).unwrap();
    let third = service.live(&receiver, &engine, 2);
    assert!(!Arc::ptr_eq(&first.config, &third.config));
    assert!(!Arc::ptr_eq(&first.pose, &third.pose));
    assert!(!Arc::ptr_eq(
        &first.devices[0].metadata,
        &third.devices[0].metadata
    ));
    assert!(first.config.yaml["custom"].is_null());
    assert_eq!(first.pose.at_ms, 0);
    assert!(first.devices[0].display_name.is_none());
    assert!(first.devices[0].sensors[0].1.rotation.is_none());
    assert!(third.devices[0].sensors[0].1.rotation.is_some());
    assert_eq!(third.devices[0].display_name.as_deref(), Some("New name"));
    receiver.forget_device("02:00:00:00:00:01");
    assert!(service.live(&receiver, &engine, 3).devices.is_empty());
    assert!(service.device_metadata.is_empty());
}
