use slimevr_core::{EventKind, Quaternion, SensorStatus, Vector3};
use slimevr_server::{
    protocol::{self, Packet},
    receiver::{Receiver, ReceiverConfig},
    recording::{self, Record, Recorder},
};
use std::{io::Write, net::SocketAddr};

const MAC: &str = "02:00:00:00:00:01";
fn addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().unwrap()
}
fn wire(id: u32, seq: i64, body: &[u8]) -> Vec<u8> {
    [
        id.to_be_bytes().as_slice(),
        seq.to_be_bytes().as_slice(),
        body,
    ]
    .concat()
}
fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}
fn handshake(mac: [u8; 6], seq: i64, version: u32) -> Vec<u8> {
    let mut body: Vec<_> = [9u32, 13, 1, 0, 0, 0, version]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    body.extend(b"\x05good\0");
    body.extend(mac);
    wire(3, seq, &body)
}
fn accepted() -> Receiver {
    Receiver::new(ReceiverConfig {
        accept_new_devices: true,
        ..Default::default()
    })
    .unwrap()
}
fn initialized(version: u32) -> Receiver {
    let mut r = accepted();
    r.receive(addr(1111), &handshake([2, 0, 0, 0, 0, 1], 0, version), 0);
    r.receive(addr(1111), &wire(15, 1, &[0, 1, 13, 0, 0, 1, 4, 0]), 1);
    r
}
fn rotation(sensor: u8, seq: i64) -> Vec<u8> {
    let mut body = vec![sensor, 1];
    body.extend(floats(&[0.0, 0.0, 0.0, 1.0]));
    body.push(3);
    wire(17, seq, &body)
}

#[test]
fn delayed_arrival_is_diagnostic_only_and_is_not_refreshed_by_other_sensor_or_acceleration() {
    let mut r = initialized(22);
    r.receive(addr(1111), &wire(15, 2, &[1, 1, 13]), 2);
    let fx = r.receive_timed(addr(1111), &rotation(0, 3), 200, Some(100));
    let sample = fx
        .events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::Sample { sample } => Some(sample),
            _ => None,
        })
        .unwrap();
    assert_eq!(sample.received_at_ms, 200);
    assert_eq!(sample.socket_received_at_ms, Some(100));
    r.receive_timed(addr(1111), &rotation(1, 4), 210, Some(205));
    r.receive_timed(addr(1111), &wire(4, 5, &[0; 13]), 220, Some(215));
    r.receive_timed(addr(1111), &rotation(0, 3), 230, Some(229)); // Rejected duplicate.
    assert_eq!(r.devices[MAC].last_alive_ms, 220);
    assert_eq!(
        r.devices[MAC].sensors[&0]
            .rotation
            .as_ref()
            .unwrap()
            .received_at_ms,
        200
    );
    let s = r.snapshot(250);
    assert_eq!(s["freshness"][0]["pose_age_ms"], 150);
    assert_eq!(s["freshness"][0]["pose_processing_age_ms"], 50);
    assert_eq!(s["freshness"][0]["pose_queue_delay_ms"], 100);
    assert_eq!(s["freshness"][1]["pose_age_ms"], 45);
    r.tick(1200); // Transport timeout uses the state machine's processing heartbeat.
    assert!(!r.devices[MAC].transport_timed_out);
    r.receive_timed(
        addr(1111),
        &handshake([2, 0, 0, 0, 0, 1], 0, 22),
        1300,
        Some(1200),
    );
    assert!(r.devices[MAC].sensors.is_empty());
    r.receive_timed(addr(1111), &wire(15, 1, &[0, 1, 13]), 1301, Some(1300));
    assert!(r.devices[MAC].sensors[&0].rotation.is_none());
    assert!(r.devices[MAC].sensors[&0].udp_rotation_timing.is_none());
}

#[test]
fn dual_clock_journal_replays_exactly_with_arrival_before_an_earlier_processing_tick() {
    use slimevr_core::{
        pose::{PoseConfig, PoseEngine, TrackerBinding},
        skeleton::BodyPosition,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dual-clock.jsonl");
    let mut r = accepted();
    let pose_config = PoseConfig {
        bindings: vec![TrackerBinding {
            device_key: MAC.into(),
            sensor_id: 0,
            body: BodyPosition::Chest,
            mounting: Quaternion::IDENTITY,
        }],
        ..Default::default()
    };
    let mut engine = PoseEngine::new(pose_config.clone()).unwrap();
    let mut journal = Recorder::create(&path, &r.config).unwrap();
    for (at, received, bytes) in [
        (0, 0, handshake([2, 0, 0, 0, 0, 1], 0, 22)),
        (1, 1, wire(15, 1, &[0, 1, 13])),
        (200, 100, rotation(0, 2)),
    ] {
        if at == 200 {
            journal.write(&Record::Tick { at_ms: 190 }).unwrap();
            assert!(r.tick(190).outbound.is_empty());
            engine.tick(190).unwrap();
        }
        journal
            .write(&Record::Receive {
                at_ms: at,
                received_at_ms: Some(received),
                from: addr(1111),
                hex: recording::encode_hex(&bytes),
            })
            .unwrap();
        let fx = r.receive_timed(addr(1111), &bytes, at, Some(received));
        for event in &fx.events {
            engine.ingest(event).unwrap();
        }
        for reply in fx.outbound {
            journal
                .write(&Record::Send {
                    at_ms: at,
                    to: reply.to,
                    hex: recording::encode_hex(&reply.bytes),
                })
                .unwrap();
        }
    }
    journal.finish(250).unwrap();
    let replay = recording::replay(&path, |_| {}).unwrap();
    assert_eq!(r.snapshot(250), replay.receiver.snapshot(250));
    engine.tick(250).unwrap();
    let mut replay_engine = PoseEngine::new(pose_config).unwrap();
    recording::replay_observed(&path, |input| {
        match input {
            recording::ReplayInput::Event(event) => replay_engine.ingest(event)?,
            recording::ReplayInput::Tick(at) => {
                replay_engine.tick(at)?;
            }
            _ => {}
        }
        Ok(())
    })
    .unwrap();
    replay_engine.tick(250).unwrap();
    assert_eq!(
        serde_json::to_value(engine.snapshot()).unwrap(),
        serde_json::to_value(replay_engine.snapshot()).unwrap()
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let future = text.replace("\"received_at_ms\":100", "\"received_at_ms\":201");
    std::fs::write(&path, future).unwrap();
    assert!(recording::replay(&path, |_| {})
        .err()
        .unwrap()
        .to_string()
        .contains("receive clock"));
}

#[test]
fn udp_rehandshake_after_server_stall_preserves_calibration_and_rejects_old_session() {
    use slimevr_core::{
        calibration::ResetKind,
        filtering::FilterType,
        pose::{PoseConfig, PoseEngine, TrackerBinding},
        skeleton::BodyPosition,
    };
    let mut config = PoseConfig {
        bindings: vec![TrackerBinding {
            device_key: MAC.into(),
            sensor_id: 0,
            body: BodyPosition::Chest,
            mounting: Quaternion::IDENTITY,
        }],
        ..Default::default()
    };
    config.filter.mode = FilterType::None;
    let mut engine = PoseEngine::new(config).unwrap();
    let mut receiver = initialized(22);
    let first = receiver.receive(addr(1111), &rotation(0, 2), 2);
    for event in &first.events {
        engine.ingest(event).unwrap();
    }
    let old_sample = first
        .events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::Sample { sample } => Some(sample.clone()),
            _ => None,
        })
        .unwrap();
    engine.reset(3, ResetKind::Full).unwrap();
    engine.reset(4, ResetKind::Mounting).unwrap();
    let before = engine.tick(4).unwrap().clone();
    assert!(before.trackers[0].calibration.full_reset_done);
    assert!(before.trackers[0].calibration.mounting_reset_done);

    // Firmware can retry its handshake when a stalled server misses keepalives.
    // Also exercise a changed endpoint, as Java reuses the MAC-stable tracker.
    for event in receiver
        .receive(addr(2222), &handshake([2, 0, 0, 0, 0, 1], 0, 22), 4000)
        .events
    {
        engine.ingest(&event).unwrap();
    }
    let after = engine.tick(4000).unwrap();
    assert!(
        after.trackers[0].calibration.full_reset_done,
        "transport retry erased full reset"
    );
    assert!(after.trackers[0].calibration.mounting_reset_done);
    assert!(
        after.trackers[0].raw.is_none(),
        "reconnect must await fresh orientation"
    );
    let mut stale = old_sample;
    stale.received_at_ms = 4001;
    engine.sample(&stale).unwrap();
    assert_eq!(engine.tick(4001).unwrap().ignored_samples, 1);

    for event in receiver
        .receive(addr(2222), &wire(15, 1, &[0, 1, 13]), 4002)
        .events
    {
        engine.ingest(&event).unwrap();
    }
    for event in receiver.receive(addr(2222), &rotation(0, 2), 4003).events {
        engine.ingest(&event).unwrap();
    }
    let restored = engine.tick(4003).unwrap();
    assert_eq!(restored.trackers[0].session, 2);
    assert!(
        restored.trackers[0]
            .rotation
            .unwrap()
            .angle_to_r(before.trackers[0].rotation.unwrap())
            < 1e-5
    );
    assert_eq!(
        serde_json::to_value(&restored.trackers[0].calibration).unwrap(),
        serde_json::to_value(&before.trackers[0].calibration).unwrap()
    );
}

#[test]
fn modern_handshake_and_ack_have_legacy_reply_layouts() {
    let mut r = accepted();
    let fx = r.receive(addr(1111), &handshake([2, 0, 0, 0, 0, 1], 50, 22), 0);
    assert_eq!(fx.outbound[0].bytes.len(), 64);
    assert_eq!(&fx.outbound[0].bytes[..13], b"\x03Hey OVR =D 5");
    assert!(fx.outbound[0].bytes[13..].iter().all(|b| *b == 0));
    assert_eq!(r.devices[MAC].handshake.firmware.as_deref(), Some("good"));
    assert_eq!(r.devices[MAC].handshake.imu_type, 13);
    assert!(r.devices[MAC].sensors.is_empty());
    assert_eq!(r.devices[MAC].last_sequence, 0);
    let fx = r.receive(addr(1111), &wire(15, 1, &[1, 1]), 1);
    assert_eq!(fx.outbound[0].bytes, vec![0, 0, 0, 15, 1, 1]);
    let fx = r.receive(addr(1111), &wire(22, 2, &[7]), 2);
    assert_eq!(
        fx.outbound[0].bytes,
        vec![0, 0, 0, 22, 0, 0, 0, 0, 0, 0, 0, 0, 3]
    );
}

#[test]
fn admission_is_explicit_and_does_not_grow_pending_state() {
    let mut r = Receiver::new(ReceiverConfig::default()).unwrap();
    for i in 0..200 {
        let fx = r.receive(
            addr(1111 + i),
            &handshake([2, 0, 0, 0, 0, 1], 0, 22),
            i as u64,
        );
        assert!(fx.outbound.is_empty());
    }
    assert!(r.devices.is_empty());
    assert_eq!(r.counters.admission_denied, 200);
    let mut r = Receiver::new(ReceiverConfig {
        allowed_macs: vec!["02-00-00-00-00-01".into()],
        ..Default::default()
    })
    .unwrap();
    assert!(!r
        .receive(addr(1111), &handshake([2, 0, 0, 0, 0, 1], 0, 22), 0)
        .outbound
        .is_empty());
    assert!(r
        .receive(addr(1112), &handshake([2, 0, 0, 0, 0, 2], 0, 22), 1)
        .outbound
        .is_empty());
    let mut limited = Receiver::new(ReceiverConfig {
        accept_new_devices: true,
        max_devices: 1,
        ..Default::default()
    })
    .unwrap();
    limited.receive(addr(1111), &handshake([2, 0, 0, 0, 0, 1], 0, 22), 0);
    assert!(limited
        .receive(addr(1112), &handshake([2, 0, 0, 0, 0, 2], 0, 22), 1)
        .outbound
        .is_empty());
    assert!(!limited
        .receive(addr(2222), &handshake([2, 0, 0, 0, 0, 1], 0, 22), 2)
        .outbound
        .is_empty());
    assert_eq!(limited.devices.len(), 1);
}

#[test]
fn duplicate_reordering_gaps_and_sequence_zero_are_separate() {
    let mut r = initialized(22);
    r.receive(addr(1111), &rotation(0, 5), 10);
    let before = r.devices[MAC].sensors[&0].rotation.clone();
    r.receive(addr(1111), &rotation(0, 5), 11);
    r.receive(addr(1111), &rotation(0, 4), 12);
    assert_eq!(r.devices[MAC].sensors[&0].rotation, before);
    assert_eq!(r.devices[MAC].last_alive_ms, 10);
    assert_eq!(r.counters.sequence_rejected, 2);
    assert_eq!(r.counters.sequence_gaps, 3);
    r.receive(addr(1111), &rotation(0, 0), 13);
    r.receive(addr(1111), &rotation(0, 1), 14);
    assert_eq!(r.devices[MAC].sensors[&0].samples, 3);
}

#[test]
fn reboot_and_mac_stable_address_change_start_clean_sessions() {
    let mut r = initialized(22);
    r.receive(addr(1111), &rotation(0, 100), 10);
    r.receive(addr(1111), &handshake([2, 0, 0, 0, 0, 1], 1, 22), 20);
    assert_eq!(r.devices[MAC].session, 2);
    assert!(r.devices[MAC].sensors.is_empty());
    r.receive(addr(1111), &wire(15, 1, &[0, 1]), 21);
    r.receive(addr(1111), &rotation(0, 2), 22);
    assert_eq!(r.devices[MAC].sensors[&0].samples, 1);
    r.receive(addr(2222), &handshake([2, 0, 0, 0, 0, 1], 0, 22), 30);
    assert_eq!(r.devices.len(), 1);
    assert_eq!(r.devices[MAC].session, 3);
    let fx = r.receive(addr(1111), &rotation(0, 200), 31);
    assert!(fx
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::Ignored { .. })));
    assert_eq!(r.devices[MAC].last_alive_ms, 30);
}

#[test]
fn shared_ip_and_endpoint_reuse_do_not_mix_device_streams() {
    let mut r = initialized(22);
    r.receive(addr(2222), &handshake([2, 0, 0, 0, 0, 2], 0, 22), 2);
    r.receive(addr(2222), &wire(15, 1, &[0, 1]), 3);
    r.receive(addr(2222), &rotation(0, 2), 4);
    r.receive(addr(1111), &rotation(0, 2), 5);
    assert_eq!(r.devices.len(), 2);
    assert_eq!(
        r.devices[MAC].sensors[&0]
            .rotation
            .as_ref()
            .unwrap()
            .received_at_ms,
        5
    );
    // Device two takes one's endpoint, then one reconnects elsewhere.
    r.receive(addr(1111), &handshake([2, 0, 0, 0, 0, 2], 0, 22), 6);
    r.tick(7);
    assert!(r.devices[MAC].transport_timed_out);
    r.receive(addr(3333), &handshake([2, 0, 0, 0, 0, 1], 0, 22), 8);
    r.receive(addr(1111), &wire(15, 1, &[0, 1]), 9);
    assert!(r.devices["02:00:00:00:00:02"].sensors.contains_key(&0));
}

#[test]
fn heartbeat_liveness_is_independent_from_pose_freshness_and_ping() {
    let mut r = initialized(22);
    r.receive(addr(1111), &rotation(0, 2), 10);
    for t in [500, 1000, 1500, 2000, 2500] {
        let fx = r.tick(t);
        let ping = fx
            .outbound
            .iter()
            .find(|p| p.bytes[..4] == 10u32.to_be_bytes())
            .unwrap();
        let id = &ping.bytes[12..];
        r.receive(addr(1111), &wire(10, 0, id), t + 10);
    }
    r.tick(2600);
    let snapshot = r.snapshot(2600);
    assert!(!r.devices[MAC].transport_timed_out);
    assert_eq!(r.devices[MAC].sensors[&0].status, SensorStatus::Ok);
    assert_eq!(r.devices[MAC].half_rtt_ms.as_ref().unwrap().value, 5);
    assert_eq!(snapshot["freshness"][0]["pose_stale"], true);
    assert_eq!(r.devices[MAC].sensors[&0].samples, 1);
    r.tick(3600);
    assert!(r.devices[MAC].transport_timed_out);
    r.tick(4600);
    assert_eq!(r.devices[MAC].sensors[&0].status, SensorStatus::TimedOut);
    r.tick(7600);
    assert_eq!(
        r.devices[MAC].sensors[&0].status,
        SensorStatus::Disconnected
    );
    r.receive(addr(1111), &wire(0, 0, &[]), 7700);
    assert_eq!(
        r.devices[MAC].sensors[&0].status,
        SensorStatus::Disconnected
    );
    r.receive(addr(1111), &rotation(0, 1), 7701);
    assert_eq!(r.devices[MAC].sensors[&0].status, SensorStatus::Ok);
}

#[test]
fn malformed_bundle_has_no_partial_state_or_liveness_updates() {
    let mut r = initialized(22);
    let before = r.snapshot(10)["devices"].clone();
    let sensor_body = [0, 0, 0, 15, 1, 1];
    let mut bundle = vec![0, 6];
    bundle.extend(sensor_body);
    bundle.extend([0, 100, 0]);
    let fx = r.receive(addr(1111), &wire(100, 50, &bundle), 20);
    assert!(fx.outbound.is_empty());
    assert_eq!(r.snapshot(10)["devices"], before);
    assert_eq!(r.counters.malformed, 1);
    // A following valid lower sequence must still be accepted.
    r.receive(addr(1111), &rotation(0, 2), 21);
    assert_eq!(r.devices[MAC].sensors[&0].samples, 1);
}

#[test]
fn normal_and_compact_bundles_support_two_sensors_without_cross_updates() {
    let mut r = initialized(22);
    let mut body = vec![0, 6];
    body.extend([0, 0, 0, 15, 1, 1]);
    body.extend([0, 4, 0, 0, 3, 231]); // unknown packet 999
    r.receive(addr(1111), &wire(100, 2, &body), 2);
    let mut compact = vec![0]; // zero-length entries are permitted.
    let rot = rotation(0, 3);
    compact.push((rot.len() - 11) as u8);
    compact.push(17);
    compact.extend(&rot[12..]);
    let mut packed = vec![23, 1];
    for n in [0i16, 0, 0, 32767, 128, -256, 384] {
        packed.extend(n.to_be_bytes());
    }
    compact.push(packed.len() as u8);
    compact.extend(packed);
    let fx = r.receive(addr(1111), &wire(101, 3, &compact), 3);
    assert_eq!(r.devices[MAC].sensors[&0].samples, 1);
    assert_eq!(r.devices[MAC].sensors[&1].samples, 1);
    assert_eq!(
        r.devices[MAC].sensors[&1]
            .acceleration
            .as_ref()
            .unwrap()
            .value,
        Vector3 {
            x: 1.0,
            y: -2.0,
            z: 3.0
        }
    );
    assert_eq!(
        fx.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Sample { .. }))
            .count(),
        2
    );
}

#[test]
fn protocol_22_acceleration_and_axes_adaptation_do_not_double_mount() {
    for version in [21, 22] {
        let mut r = initialized(version);
        let fx = r.receive(addr(1111), &rotation(0, 2), 2);
        let EventKind::Sample { sample } = &fx.events.last().unwrap().kind else {
            panic!()
        };
        assert_eq!(sample.packet_rotation, Some(Quaternion::IDENTITY));
        assert_eq!(sample.sensor_timestamp_us, None);
        let q = sample.server_rotation.unwrap();
        assert!((q.w - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((q.x + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        let mut body = floats(&[1.0, 2.0, 3.0]);
        body.push(0);
        r.receive(addr(1111), &wire(4, 3, &body), 3);
        let expected = if version == 22 {
            Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }
        } else {
            Vector3 {
                x: 2.0,
                y: -1.0,
                z: 3.0,
            }
        };
        assert_eq!(
            r.devices[MAC].sensors[&0]
                .acceleration
                .as_ref()
                .unwrap()
                .value,
            expected
        );
    }
}

#[test]
fn absent_telemetry_is_unknown_and_observed_values_keep_freshness() {
    let mut r = initialized(22);
    let d = &r.devices[MAC];
    assert!(d.rssi.is_none() && d.battery_voltage.is_none() && d.sensors[&0].temperature.is_none());
    r.receive(addr(1111), &wire(19, 2, &[0, (-71i8) as u8]), 10);
    let mut temp = vec![0];
    temp.extend(floats(&[35.5]));
    r.receive(addr(1111), &wire(20, 3, &temp), 20);
    r.receive(addr(1111), &wire(12, 4, &floats(&[3.9, 0.75])), 30);
    r.tick(500);
    let d = &r.devices[MAC];
    assert_eq!(d.rssi.as_ref().unwrap().value, -71);
    assert_eq!(
        d.sensors[&0].temperature.as_ref().unwrap().received_at_ms,
        20
    );
    assert_eq!(d.battery_fraction.as_ref().unwrap().received_at_ms, 30);
}

#[test]
fn unknown_sensor_and_correction_frames_do_not_create_pose() {
    let mut r = initialized(22);
    r.receive(addr(1111), &rotation(1, 2), 2);
    let mut correction = rotation(0, 3);
    correction[13] = 2;
    r.receive(addr(1111), &correction, 3);
    assert_eq!(r.devices[MAC].sensors.len(), 1);
    assert!(r.devices[MAC].sensors[&0].rotation.is_none());
}

#[test]
fn finite_and_compatibility_rules_handle_invalid_float_samples() {
    let mut body = vec![0, 1];
    body.extend(floats(&[f32::NAN, 0.0, 0.0, 1.0]));
    body.push(0);
    let p = protocol::parse(&wire(17, 1, &body)).unwrap();
    assert!(matches!(
        p.packets[0],
        Packet::Rotation {
            fallback: true,
            rotation: Quaternion::IDENTITY,
            ..
        }
    ));
    let mut infinite = vec![0, 1];
    infinite.extend(floats(&[f32::INFINITY, 0.0, 0.0, 1.0]));
    infinite.push(0);
    assert!(protocol::parse(&wire(17, 1, &infinite)).is_err());
    let mut overflow = vec![0, 1];
    overflow.extend(floats(&[f32::MAX, 0.0, 0.0, f32::MAX]));
    overflow.push(0);
    assert!(protocol::parse(&wire(17, 1, &overflow)).is_err());
    assert!(protocol::parse(&wire(23, 1, &[0; 15])).is_err());
}

#[test]
fn arbitrary_and_truncated_inputs_never_panic_or_allocate_unbounded_packets() {
    let fixtures = [
        handshake([2, 0, 0, 0, 0, 1], 1, 22),
        rotation(0, 2),
        wire(100, 3, &[0, 6, 0, 0, 0, 15, 0, 1]),
    ];
    for f in &fixtures {
        for end in 0..f.len() {
            let _ = protocol::parse(&f[..end]);
        }
    }
    let mut seed = 0x12345678u32;
    for length in 0..=1800 {
        let mut bytes = vec![0; length];
        for b in &mut bytes {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (seed >> 24) as u8;
        }
        if length >= 4 {
            bytes[..4].copy_from_slice(&(if length % 2 == 0 { 100u32 } else { 101 }).to_be_bytes());
        }
        let _ = protocol::parse(&bytes);
    }
}

#[test]
fn journal_roundtrip_verifies_replies_and_final_state_and_detects_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture.jsonl");
    let mut r = accepted();
    let mut journal = Recorder::create(&path, &r.config).unwrap();
    for (at, bytes) in [
        (0, handshake([2, 0, 0, 0, 0, 1], 0, 22)),
        (1, wire(15, 1, &[0, 1])),
        (2, rotation(0, 2)),
    ] {
        journal
            .write(&Record::Receive {
                received_at_ms: None,
                at_ms: at,
                from: addr(1111),
                hex: recording::encode_hex(&bytes),
            })
            .unwrap();
        let fx = r.receive(addr(1111), &bytes, at);
        for p in fx.outbound {
            journal
                .write(&Record::Send {
                    at_ms: at,
                    to: p.to,
                    hex: recording::encode_hex(&p.bytes),
                })
                .unwrap();
        }
    }
    journal.write(&Record::Tick { at_ms: 6000 }).unwrap();
    let fx = r.tick(6000);
    for p in fx.outbound {
        journal
            .write(&Record::Send {
                at_ms: 6000,
                to: p.to,
                hex: recording::encode_hex(&p.bytes),
            })
            .unwrap();
    }
    journal.finish(6000).unwrap();
    let replay = recording::replay(&path, |_| {}).unwrap();
    assert_eq!(replay.receiver.snapshot(6000), r.snapshot(6000));
    assert_eq!(replay.verified_replies, 6);
    assert!(Recorder::create(&path, &r.config).is_err()); // Never overwrite an existing capture.
    let text = std::fs::read_to_string(&path).unwrap();
    let missing_end = dir.path().join("missing-end.jsonl");
    let prefix = text.rsplit_once("{\"record\":\"end\"").unwrap().0;
    std::fs::write(&missing_end, prefix).unwrap();
    assert!(recording::replay(&missing_end, |_| {}).is_err());
    let corrupt = dir.path().join("corrupt.jsonl");
    std::fs::write(&corrupt, text.replacen("03486579", "04486579", 1)).unwrap();
    assert!(recording::replay(&corrupt, |_| {}).is_err());
    let truncated = dir.path().join("truncated.jsonl");
    std::fs::write(&truncated, &text[..text.len() - 1]).unwrap();
    assert!(recording::replay(&truncated, |_| {}).is_err());
    let mut backwards = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(backwards, "{{\"record\":\"tick\",\"at_ms\":0}}").unwrap();
    assert!(recording::replay(&path, |_| {}).is_err());
}

#[test]
fn journal_rejects_empty_bad_version_and_missing_replies() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("bad.jsonl");
    std::fs::write(&p, "").unwrap();
    assert!(recording::replay(&p, |_| {}).is_err());
    std::fs::write(&p, "{\"record\":\"header\",\"version\":99,\"config\":{}}\n").unwrap();
    assert!(recording::replay(&p, |_| {}).is_err());
    let p = dir.path().join("missing.jsonl");
    let mut journal = Recorder::create(&p, &accepted().config).unwrap();
    journal
        .write(&Record::Receive {
            received_at_ms: None,
            at_ms: 0,
            from: addr(1111),
            hex: recording::encode_hex(&handshake([2, 0, 0, 0, 0, 1], 0, 22)),
        })
        .unwrap();
    journal.flush().unwrap();
    assert!(recording::replay(&p, |_| {}).is_err());
}
