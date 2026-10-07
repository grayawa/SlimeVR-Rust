use slimevr_core::{
    pose::{PoseConfig, PoseEngine, SceneInput, TrackerBinding},
    skeleton::BodyPosition as B,
    EventKind as E, InputEvent, Quaternion as Q, Vector3 as V,
};
use slimevr_server::{
    api::{FrontendConfig, Service},
    osc::{Settings, State},
    receiver::{Receiver, ReceiverConfig},
    steamvr::{
        envelope,
        messages::{self, protobuf_message::Message as M},
        Event, Session,
    },
};
use tokio::sync::{broadcast, mpsc};
fn added(serial: &str, role: i32) -> messages::ProtobufMessage {
    envelope(M::TrackerAdded(messages::TrackerAdded {
        tracker_id: 6,
        tracker_serial: serial.into(),
        tracker_role: role,
        ..Default::default()
    }))
}
#[test]
fn vive_role_yaml_assignment_no_imu_mounting_and_recorded_reconnect() {
    let mut config=slimevr_server::config::from_yaml(serde_yaml_ng::from_str("version: 15\ntrackers:\n  LHR-test:\n    designation: body:left_foot\n    customName: Vive foot\n").unwrap()).unwrap();
    config.pose.filter.mode = slimevr_core::filtering::FilterType::Prediction;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("vrconfig.yml");
    let (tx, _) = mpsc::channel(32);
    let (events, _) = broadcast::channel(32);
    let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
    let mut receiver = Receiver::new(ReceiverConfig::default()).unwrap();
    let mut service = Service::new(config, Some(path.clone()), tx, events);
    let mut session = Session::default();
    session.receive(Event::Connected(4), 0).unwrap();
    for input in session
        .receive(Event::Message(4, added("LHR-test", 1)), 1)
        .unwrap()
    {
        if let SceneInput::Input { event } = input {
            service
                .source_input(event, &mut receiver, &mut engine)
                .unwrap();
        }
    }
    assert_eq!(service.config.pose.bindings[0].body, B::LeftFoot);
    assert_eq!(
        service.config.tracker_names["steamvr:LHR-test/0"],
        "Vive foot"
    );
    let rotation = Q::rotation_x(0.35);
    let p = messages::Position {
        tracker_id: 6,
        qw: rotation.w,
        qx: rotation.x,
        x: Some(1.0),
        y: Some(0.0),
        z: Some(2.0),
        ..Default::default()
    };
    for input in session
        .receive(Event::Message(4, envelope(M::Position(p))), 2)
        .unwrap()
    {
        if let SceneInput::Input { event } = input {
            service
                .source_input(event, &mut receiver, &mut engine)
                .unwrap();
        }
    }
    engine.tick(2).unwrap();
    let pose = &engine.snapshot().trackers[0];
    assert!(pose.rotation.unwrap().angle_to_r(rotation) < 1e-5);
    engine
        .reset(3, slimevr_core::calibration::ResetKind::Mounting)
        .unwrap();
    assert!(
        !engine.snapshot().trackers[0]
            .calibration
            .mounting_reset_done
    );
    let reloaded = FrontendConfig::load(&path).unwrap();
    assert_eq!(reloaded.pose.bindings[0].device_key, "steamvr:LHR-test");
    assert_eq!(
        reloaded.yaml["trackers"]["LHR-test"]["designation"].as_str(),
        Some("body:left_foot")
    );
    for input in session
        .receive(Event::Connected(5), 4)
        .unwrap()
        .into_iter()
        .chain(
            session
                .receive(Event::Message(5, added("LHR-test", 1)), 5)
                .unwrap(),
        )
        .chain(
            session
                .receive(Event::Message(5, envelope(M::Position(p))), 6)
                .unwrap(),
        )
    {
        if let SceneInput::Input { event } = input {
            service
                .source_input(event, &mut receiver, &mut engine)
                .unwrap();
        }
    }
    engine.tick(6).unwrap();
    assert_eq!(engine.snapshot().trackers[0].session, 5);
    assert_eq!(receiver.devices["steamvr:LHR-test"].session, 5);
}
fn packet(addr: &str, args: Vec<rosc::OscType>) -> Vec<u8> {
    rosc::encoder::encode(&rosc::OscPacket::Message(rosc::OscMessage {
        addr: addr.into(),
        args,
    }))
    .unwrap()
}
fn floats(values: &[f32]) -> Vec<rosc::OscType> {
    values.iter().copied().map(rosc::OscType::Float).collect()
}
fn sample(events: &[InputEvent]) -> &slimevr_core::TrackerSample {
    events
        .iter()
        .find_map(|e| {
            if let E::Sample { sample } = &e.kind {
                Some(sample)
            } else {
                None
            }
        })
        .unwrap()
}
#[test]
fn vrc_pose_offset_vmc_conversion_limits_and_reset_permissions() {
    let mut config = Settings::default();
    config.vrc.endpoint.enabled = true;
    config.vmc.endpoint.enabled = true;
    let mut state = State::new(config.clone()).unwrap();
    let events = state
        .receive(
            9001,
            &packet(
                "/tracking/vrsystem/head/pose",
                floats(&[1.0, 1.7, 3.0, 0.0, 90.0, 0.0]),
            ),
            1000,
            None,
        )
        .unwrap();
    let head = sample(&events);
    assert_eq!(head.position, Some(V::new(1.0, 1.7, -3.0)));
    assert!(
        head.server_rotation
            .unwrap()
            .angle_to_r(Q::rotation_y(-std::f32::consts::FRAC_PI_2))
            < 1e-5
    );
    let binding = TrackerBinding {
        device_key: head.device_key.clone(),
        sensor_id: 0,
        body: B::Head,
        mounting: Q::rotation_y(std::f32::consts::PI),
    };
    let mut engine = PoseEngine::new(PoseConfig {
        bindings: vec![binding],
        ..Default::default()
    })
    .unwrap();
    for e in events {
        engine.ingest(&e).unwrap();
    }
    engine.tick(1000).unwrap();
    engine
        .reset(1001, slimevr_core::calibration::ResetKind::Full)
        .unwrap();
    engine.tick(1002).unwrap();
    assert!(
        engine.snapshot().trackers[0]
            .rotation
            .unwrap()
            .angle_to_r(Q::rotation_y(-std::f32::consts::FRAC_PI_2))
            < 1e-5
    );
    assert!(engine.snapshot().skeleton.world_anchor_present);
    state
        .receive(
            9001,
            &packet("/tracking/trackers/head/position", floats(&[2.0, 3.0, 4.0])),
            1003,
            Some(slimevr_core::skeleton::HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(10.0, 20.0, 30.0)),
            }),
        )
        .unwrap();
    let events = state
        .receive(
            9001,
            &packet("/tracking/trackers/1/position", floats(&[3.0, 4.0, 5.0])),
            1004,
            None,
        )
        .unwrap();
    assert_eq!(sample(&events).position, Some(V::new(11.0, 21.0, 29.0)));
    let mut args = vec![rosc::OscType::String("Reference".into())];
    args.extend(floats(&[1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0]));
    let events = state
        .receive(39540, &packet("/VMC/Ext/Tra/Pos", args), 1005, None)
        .unwrap();
    assert_eq!(sample(&events).position, Some(V::new(1.0, 2.0, -3.0)));
    assert!(
        sample(&events)
            .server_rotation
            .unwrap()
            .angle_to_r(Q::IDENTITY)
            < 1e-5
    );
    assert!(state
        .receive(
            9001,
            &packet(
                "/tracking/trackers/1/rotation",
                floats(&[0.0, f32::NAN, 0.0])
            ),
            1006,
            None
        )
        .is_err());
    assert!(state.receive(9001, &vec![0; 8193], 1006, None).is_err());
    assert!(state
        .receive(
            9001,
            &packet("/tracking/trackers/9999/position", floats(&[0.0, 0.0, 0.0])),
            1006,
            None
        )
        .is_err());
    let output = state.output(engine.snapshot(), 1007, None).unwrap();
    let (_, rosc::OscPacket::Bundle(bundle)) =
        rosc::decoder::decode_udp(&output.vrc.unwrap()).unwrap()
    else {
        panic!()
    };
    assert!(bundle.content.iter().any(
        |p| matches!(p,rosc::OscPacket::Message(m)if m.addr=="/tracking/trackers/head/position")
    ));
    config.vrc.endpoint.enabled = false;
    assert!(!state.reconfigure(config, 1008).unwrap().is_empty());
    assert_eq!(state.generation, 2);
}
#[test]
fn vrm_v0_v1_translation_and_yxz_gimbal_roundtrip() {
    use slimevr_server::osc::armature::{vrm_offsets, Armature};
    for json in [
        r#"{"extensions":{"VRM":{"humanoid":{"humanBones":[{"bone":"hips","node":0}]}}},"nodes":[{"translation":[0,1,0]}]}"#,
        r#"{"extensions":{"VRMC_vrm":{"humanoid":{"humanBones":{"hips":{"node":0}}}}},"nodes":[{"translation":[0,1,0]}]}"#,
    ] {
        assert_eq!(vrm_offsets(json).unwrap()["Hips"], V::new(0.0, 1.0, 0.0));
        let mut a = Armature::new(false);
        assert_eq!(a.load_vrm(json).unwrap(), 1.0);
        a.update();
        assert_eq!(
            a.local_translation(Armature::bone("Hips").unwrap()),
            V::new(0.0, 1.0, 0.0)
        );
    }
    for e in [
        V::new(0.2, 0.5, -0.3),
        V::new(std::f32::consts::FRAC_PI_2, 0.5, -0.3),
        V::new(-std::f32::consts::FRAC_PI_2, -0.5, 0.3),
    ] {
        let q = Q::from_euler_yxz(e);
        assert!(q.angle_to_r(Q::from_euler_yxz(q.euler_yxz())) < 1e-5);
    }
}
#[tokio::test]
async fn real_osc_shared_router_port_and_hot_rebind_release() {
    use std::time::Duration;
    let input = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = input.local_addr().unwrap().port();
    drop(input);
    let target = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut config = Settings::default();
    config.router.enabled = true;
    config.vrc.endpoint.enabled = true;
    config.vrc.oscquery_enabled = false;
    config.router.port_in = port;
    config.vrc.endpoint.port_in = port;
    config.router.port_out = target.local_addr().unwrap().port();
    let (tx, mut requests) = mpsc::channel(32);
    let mut worker = slimevr_server::osc::Controller::start(config.clone(), tx);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let source = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let data = packet(
        "/tracking/vrsystem/head/pose",
        floats(&[0.0, 1.7, 0.0, 0.0, 0.0, 0.0]),
    );
    source.send_to(&data, ("127.0.0.1", port)).await.unwrap();
    let mut buf = [0u8; 2048];
    let len = tokio::time::timeout(Duration::from_secs(2), target.recv(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buf[..len], &data);
    let request = tokio::time::timeout(Duration::from_secs(2), requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        request,
        slimevr_server::api::Request::Osc(slimevr_server::osc::Event::Datagram {
            generation: 1,
            ..
        })
    ));
    config.router.enabled = false;
    config.vrc.endpoint.enabled = false;
    worker.configure(2, &config);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(tokio::net::UdpSocket::bind(("127.0.0.1", port))
        .await
        .is_ok());
    drop(worker);
}

#[test]
fn delayed_hotkeys_preserve_reset_order_and_mounting_window_state() {
    use slimevr_server::hotkeys::Action;
    let (commands, _) = mpsc::channel(32);
    let (events, _) = broadcast::channel(32);
    let mut service = Service::new(FrontendConfig::default(), None, commands, events);
    let mut engine = PoseEngine::new(PoseConfig::default()).unwrap();
    service.hotkey(Action::Full, 100, 0).unwrap();
    service.hotkey(Action::Yaw, 0, 1).unwrap();
    service.hotkey(Action::Pause, 150, 0).unwrap();
    service.before_tick(&mut engine, 1);
    let p = engine.tick(1).unwrap();
    assert_eq!(p.reset_count, 1);
    assert_eq!(p.last_full_reset_ms, None);
    assert!(!p.paused);
    service.before_tick(&mut engine, 100);
    let p = engine.tick(100).unwrap();
    assert_eq!(p.reset_count, 2);
    assert_eq!(p.last_full_reset_ms, Some(100));
    service.before_tick(&mut engine, 150);
    assert!(engine.tick(150).unwrap().paused);
    service.hotkey(Action::Feet, 0, 151).unwrap();
    service.before_tick(&mut engine, 151);
    let p = engine.tick(151).unwrap();
    assert!(p.feet_mounting_completed);
    assert!(!p.mounting_completed);
}
