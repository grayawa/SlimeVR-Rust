use slimevr_core::{EventKind, Quaternion as Q, SensorStatus};
use slimevr_server::{
    hid::{rotation, Event},
    receiver::{DeviceConfig, Origin, Receiver, ReceiverConfig},
    recording::{Record, Recorder},
};
fn register(id: u8, uid: u64) -> [u8; 16] {
    let mut p = [0; 16];
    p[0] = 255;
    p[1] = id;
    p[2..10].copy_from_slice(&uid.to_le_bytes());
    p
}
fn info(id: u8) -> [u8; 16] {
    let mut p = [0; 16];
    p[1] = id;
    p[2] = 73;
    p[3] = 120;
    p[4] = 100;
    p[5] = 1;
    p[6] = 9;
    p[8] = 19;
    p[9] = 2;
    p[12] = 1;
    p[13] = 2;
    p[14] = 3;
    p[15] = 55;
    p
}
fn report(session: u64, packets: &[[u8; 16]]) -> Event {
    Event::Report {
        path: "receiver".into(),
        session,
        bytes: packets.iter().flatten().copied().collect(),
    }
}
#[test]
fn hid_registration_fixed_point_telemetry_sleep_and_disconnect() {
    let mut receiver = Receiver::new(ReceiverConfig::default()).unwrap();
    let mut q = [0; 16];
    q[0] = 1;
    q[1] = 7;
    q[8..10].copy_from_slice(&32767i16.to_le_bytes());
    q[10..12].copy_from_slice(&128i16.to_le_bytes());
    assert!(receiver.hid(&report(1, &[q]), 0).events.is_empty());
    let fx = receiver.hid(&report(1, &[register(7, 0xaabbccddeeff), info(7), q]), 1);
    assert!(fx.outbound.is_empty());
    let d = &receiver.devices["hid:AABBCCDDEEFF"];
    assert_eq!(d.origin, Origin::Hid);
    assert_eq!(d.handshake.firmware.as_deref(), Some("1.2.3"));
    assert_eq!(d.rssi.as_ref().unwrap().value, -55);
    assert_eq!(d.battery_fraction.as_ref().unwrap().value, 0.73);
    assert_eq!(d.sensors[&0].temperature.as_ref().unwrap().value, 11.0);
    assert_eq!(d.sensors[&0].acceleration.as_ref().unwrap().value.x, 1.0);
    let actual = d.sensors[&0].rotation.as_ref().unwrap().value;
    let expected = Q::rotation_x(-std::f32::consts::FRAC_PI_2);
    assert!((actual.w - expected.w).abs() < 0.0001);
    assert!((actual.x - expected.x).abs() < 0.0001);
    assert!(receiver.tick(999999).outbound.is_empty());
    assert_eq!(
        receiver.devices["hid:AABBCCDDEEFF"].sensors[&0].status,
        SensorStatus::Ok
    );
    let mut data = [0; 16];
    data[0] = 6;
    data[1] = 7;
    data[2] = 1;
    data[4] = 50;
    let fx = receiver.hid(&report(1, &[data]), 1000000);
    assert!(fx
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::TapSetup { sensor_id: 0, .. })));
    assert!(!receiver
        .hid(&report(1, &[data]), 1000001)
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::TapSetup { .. })));
    receiver.tick(1000051);
    assert_eq!(
        receiver.devices["hid:AABBCCDDEEFF"].sensors[&0].status,
        SensorStatus::TimedOut
    );
    receiver.hid(&report(1, &[q]), 1000052);
    assert_eq!(
        receiver.devices["hid:AABBCCDDEEFF"].sensors[&0].status,
        SensorStatus::Ok
    );
    let effects = receiver
        .config_command(&DeviceConfig {
            device_key: "hid:AABBCCDDEEFF".into(),
            sensor_id: 0,
            config_type: 1,
            enabled: false,
        })
        .unwrap();
    assert!(effects.outbound.is_empty());
    assert_eq!(
        receiver.devices["hid:AABBCCDDEEFF"].sensors[&0].info.config,
        Some(2)
    );
    receiver.hid(
        &Event::Closed {
            path: "receiver".into(),
            session: 1,
        },
        1000053,
    );
    assert_eq!(
        receiver.devices["hid:AABBCCDDEEFF"].sensors[&0].status,
        SensorStatus::Disconnected
    );
    let before = receiver.devices["hid:AABBCCDDEEFF"].session;
    receiver.hid(
        &report(2, &[register(7, 0xaabbccddeeff), info(7), q]),
        1000054,
    );
    assert!(receiver.devices["hid:AABBCCDDEEFF"].session > before);
    receiver.hid(
        &Event::Closed {
            path: "receiver".into(),
            session: 1,
        },
        1000055,
    );
    assert!(!receiver.devices["hid:AABBCCDDEEFF"].transport_timed_out);
}
#[test]
fn compressed_rotation_and_corrupt_reports() {
    let mut data = [0; 16];
    data[0] = 2;
    let bits = 512 | (1024 << 10) | (1024 << 21);
    data[5..9].copy_from_slice(&u32::to_le_bytes(bits));
    assert_eq!(
        rotation(&data),
        Some(Q::rotation_x(-std::f32::consts::FRAC_PI_2))
    );
    let mut r = Receiver::new(Default::default()).unwrap();
    let event = Event::Report {
        path: "receiver".into(),
        session: 1,
        bytes: vec![1; 15],
    };
    assert!(matches!(
        r.hid(&event, 0).events[0].kind,
        EventKind::Rejected { .. }
    ));
    let mut zero = [0; 16];
    zero[0] = 1;
    assert!(rotation(&zero).is_none());
}
#[test]
fn native_hid_yaml_and_replay_preserve_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vrconfig.yml");
    std::fs::write(&path,"version: '15'\nhidConfig: {trackersOverHID: true}\ntrackers:\n  AABBCCDDEEFF/0:\n    designation: body:left_upper_leg\n    customName: My HID tracker\n").unwrap();
    let c = slimevr_server::api::FrontendConfig::load(&path).unwrap();
    assert_eq!(c.pose.bindings[0].device_key, "hid:AABBCCDDEEFF");
    c.save(Some(&path)).unwrap();
    let again = slimevr_server::api::FrontendConfig::load(&path).unwrap();
    assert_eq!(again.pose.bindings, c.pose.bindings);
    assert_eq!(again.tracker_names["hid:AABBCCDDEEFF/0"], "My HID tracker");
    assert!(again.yaml["rust"]["otherBindings"]
        .as_sequence()
        .unwrap()
        .is_empty());
    assert!(again.yaml["trackers"]["AABBCCDDEEFF/0"].is_mapping());
    let journal = directory.path().join("input.jsonl");
    let mut recorder = Recorder::create_version(&journal, &Default::default(), 4).unwrap();
    recorder
        .write(&Record::Hid {
            at_ms: 1,
            event: report(1, &[register(7, 0xaabbccddeeff), info(7)]),
        })
        .unwrap();
    recorder.write(&Record::Tick { at_ms: 10 }).unwrap();
    recorder.write(&Record::End { at_ms: 11 }).unwrap();
    recorder.flush().unwrap();
    let mut events = Vec::new();
    slimevr_server::recording::replay(&journal, |e| events.push(e.clone())).unwrap();
    assert!(events.iter().any(|e|matches!(&e.kind,EventKind::DeviceConnected{device_key,..}if device_key=="hid:AABBCCDDEEFF")));
}
