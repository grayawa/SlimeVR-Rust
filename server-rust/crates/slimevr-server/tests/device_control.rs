use slimevr_server::{
    api::{device_control::Controller, FrontendConfig},
    receiver::{DeviceConfig, Receiver, ReceiverConfig},
    recording::{self, Record, Recorder},
};
const MAC: &str = "02:00:00:00:00:01";
fn wire(id: u32, seq: i64, body: &[u8]) -> Vec<u8> {
    [
        id.to_be_bytes().as_slice(),
        seq.to_be_bytes().as_slice(),
        body,
    ]
    .concat()
}
fn init() -> Receiver {
    let mut r = Receiver::new(ReceiverConfig {
        accept_new_devices: true,
        ..Default::default()
    })
    .unwrap();
    let mut body: Vec<u8> = [9u32, 13, 1, 0, 0, 0, 22]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    body.extend(b"\x05good\0\x02\0\0\0\0\x01");
    r.receive("127.0.0.1:1111".parse().unwrap(), &wire(3, 0, &body), 0);
    r.receive(
        "127.0.0.1:1111".parse().unwrap(),
        &wire(15, 1, &[0, 1, 13, 0, 2, 1, 0, 0]),
        1,
    );
    r
}
fn command(enabled: bool) -> DeviceConfig {
    DeviceConfig {
        device_key: MAC.into(),
        sensor_id: 0,
        config_type: 1,
        enabled,
    }
}
fn ack(r: &mut Receiver, seq: i64) {
    r.receive(
        "127.0.0.1:1111".parse().unwrap(),
        &wire(24, seq, &[0, 0, 1]),
        seq as u64,
    );
}
#[test]
fn configuration_requires_ack_and_updates_reported_sensor_flags() {
    let mut r = init();
    let mut c = Controller::default();
    let config = FrontendConfig::default();
    c.apply(vec![command(true)], &mut r, 2, Some((9, None, true)))
        .unwrap();
    assert_eq!(
        c.outbound[0].1.outbound[0].bytes,
        wire(25, 0, &[0, 0, 1, 1])
    );
    assert_eq!(r.devices[MAC].sensors[&0].info.config, Some(2));
    assert!(c.tick(&mut r, &config, 3).is_empty());
    assert!(c.apply(vec![command(false)], &mut r, 3, None).is_err());
    ack(&mut r, 2);
    assert_eq!(r.devices[MAC].sensors[&0].info.config, Some(3));
    assert_eq!(
        c.tick(&mut r, &config, 4),
        vec![(Some((9, None, true)), None)]
    );
}
#[test]
fn timeout_fences_late_ack_and_reboot_reapplies_native_preferences() {
    let mut r = init();
    let mut c = Controller::default();
    let mut config = FrontendConfig {
        magnetometers_enabled: true,
        ..Default::default()
    };
    c.tick(&mut r, &config, 2);
    assert_eq!(c.outbound.len(), 1);
    assert!(c.tick(&mut r, &config, 10002)[0]
        .1
        .as_ref()
        .unwrap()
        .contains("10 seconds"));
    assert!(c.ready(&[command(false)]).is_err());
    ack(&mut r, 2);
    c.tick(&mut r, &config, 10003);
    assert!(c.ready(&[command(false)]).is_ok());
    config.mag_preferences.insert(format!("{MAC}/0"), false);
    c.tick(&mut r, &config, 10004);
    assert_eq!(c.outbound.len(), 2);
    assert!(!c.outbound[1].0.enabled);
    // A new firmware session cannot acknowledge an old pending operation.
    let mut reboot = init();
    reboot.devices.get_mut(MAC).unwrap().session = r.devices[MAC].session + 1;
    let completed = c.tick(&mut reboot, &config, 10005);
    assert!(completed[0].1.as_ref().unwrap().contains("restarted"));
    config.mag_preferences.clear();
    c.tick(&mut reboot, &config, 10006);
    assert_eq!(c.outbound.len(), 3);
}
#[test]
fn invalid_group_is_rejected_without_partial_output_and_offline_sensors_are_excluded() {
    let mut r = init();
    let mut c = Controller::default();
    let mut unknown = command(false);
    unknown.device_key = "unknown".into();
    assert!(c
        .apply(vec![command(true), unknown], &mut r, 2, None)
        .is_err());
    assert!(c.outbound.is_empty());
    r.devices.get_mut(MAC).unwrap().transport_timed_out = true;
    assert!(c
        .tick(
            &mut r,
            &FrontendConfig {
                magnetometers_enabled: true,
                ..Default::default()
            },
            3
        )
        .is_empty());
    assert!(c.outbound.is_empty());
}
#[test]
fn device_command_journal_replays_ack_state_and_exact_udp_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    let config = ReceiverConfig {
        accept_new_devices: true,
        ..Default::default()
    };
    let mut r = Receiver::new(config.clone()).unwrap();
    let mut journal = Recorder::create_version(&path, &config, 3).unwrap();
    let setup = init();
    let d = &setup.devices[MAC];
    let mut body: Vec<u8> = [9u32, 13, 1, 0, 0, 0, 22]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    body.extend(b"\x05good\0\x02\0\0\0\0\x01");
    for (at, bytes) in [
        (0, wire(3, 0, &body)),
        (1, wire(15, 1, &[0, 1, 13, 0, 2, 1, 0, 0])),
    ] {
        journal
            .write(&Record::Receive {
                received_at_ms: None,
                at_ms: at,
                from: d.address,
                hex: recording::encode_hex(&bytes),
            })
            .unwrap();
        let fx = r.receive(d.address, &bytes, at);
        for o in fx.outbound {
            journal
                .write(&Record::Send {
                    at_ms: at,
                    to: o.to,
                    hex: recording::encode_hex(&o.bytes),
                })
                .unwrap();
        }
    }
    journal
        .write(&Record::DeviceConfig {
            at_ms: 2,
            command: command(true),
        })
        .unwrap();
    let fx = r.config_command(&command(true)).unwrap();
    for o in fx.outbound {
        journal
            .write(&Record::Send {
                at_ms: 2,
                to: o.to,
                hex: recording::encode_hex(&o.bytes),
            })
            .unwrap();
    }
    let bytes = wire(24, 2, &[0, 0, 1]);
    journal
        .write(&Record::Receive {
            received_at_ms: None,
            at_ms: 3,
            from: d.address,
            hex: recording::encode_hex(&bytes),
        })
        .unwrap();
    r.receive(d.address, &bytes, 3);
    journal.write(&Record::End { at_ms: 3 }).unwrap();
    journal.flush().unwrap();
    let replay = recording::replay(&path, |_| {}).unwrap();
    assert_eq!(
        serde_json::to_value(replay.receiver.devices).unwrap(),
        serde_json::to_value(r.devices).unwrap()
    );
}
