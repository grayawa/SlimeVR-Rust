//! An isolated benchmark of live snapshot construction and destruction costs.
use super::*;
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "Run in release with --ignored --nocapture to measure live snapshot costs"]
fn benchmark_live_snapshot_sharing() {
    let mut config = crate::config::from_yaml(
        serde_yaml_ng::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/vrconfig-v15.yml"
        )))
        .unwrap(),
    )
    .unwrap();
    config.pose.bindings.clear();
    let roles = [
        B::Chest,
        B::Hip,
        B::LeftUpperLeg,
        B::LeftLowerLeg,
        B::RightUpperLeg,
        B::RightLowerLeg,
        B::Waist,
        B::LeftFoot,
        B::RightFoot,
        B::UpperChest,
        B::LeftUpperArm,
        B::RightUpperArm,
    ];
    let mut receiver = Receiver::new(crate::receiver::ReceiverConfig {
        accept_new_devices: true,
        ..Default::default()
    })
    .unwrap();
    for id in 1u8..=6 {
        let from = format!("127.0.0.1:{}", 7000 + u16::from(id))
            .parse()
            .unwrap();
        let mut handshake: Vec<u8> = [9u32, 13, 1, 0, 0, 0, 22]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect();
        handshake.extend(b"\x05test\0");
        handshake.extend([2, 0, 0, 0, 0, id]);
        receiver.receive(from, &crate::protocol::header(3, &handshake), 0);
        let key = format!("02:00:00:00:00:{id:02X}");
        config.device_ids.insert(key.clone(), id);
        for sensor_id in 0..2 {
            receiver.receive(
                from,
                &crate::protocol::header(15, &[sensor_id, 1, 13, 0, 0, 1, 4, 0]),
                1,
            );
            config.pose.bindings.push(TrackerBinding {
                device_key: key.clone(),
                sensor_id,
                body: roles[usize::from((id - 1) * 2 + sensor_id)],
                mounting: slimevr_core::Quaternion::IDENTITY,
            });
        }
    }
    let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
    for device in receiver.devices.values() {
        for (sensor_id, sensor) in &device.sensors {
            engine
                .ingest(&InputEvent {
                    at_ms: 1,
                    kind: EventKind::SensorState {
                        device_key: device.key.clone(),
                        sensor_id: *sensor_id,
                        status: sensor.status,
                    },
                })
                .unwrap();
        }
    }
    engine.tick(1).unwrap();
    let (commands, _) = mpsc::channel(32);
    let (events, _) = broadcast::channel(64);
    let mut service = Service::new(config, None, commands, events);
    let snapshot = service.live(&receiver, &engine, 1);
    assert_eq!(snapshot.devices.len(), 6);
    assert_eq!(snapshot.pose.trackers.len(), 12);
    assert!(snapshot.devices.iter().all(|d| d.sensors.len() == 2));
    // Alternating order limits warmup/order bias; destruction is included in both.
    for trial in 0..3 {
        for shared in if trial % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut durations = Vec::with_capacity(20_000);
            for at in 0..20_000 {
                let started = Instant::now();
                if shared {
                    drop(black_box(service.live(&receiver, &engine, at)));
                } else {
                    drop(black_box(Arc::new((
                        at,
                        receiver.devices.clone(),
                        service.config.clone(),
                        engine.snapshot().clone(),
                        service.external.clone(),
                        service.state_path.is_some(),
                        service.steam_vr.clone(),
                        service.driver_status.clone(),
                    ))));
                }
                durations.push(started.elapsed().as_secs_f64() * 1e6);
            }
            durations.sort_by(f64::total_cmp);
            let percentile = |p: f64| durations[(p * durations.len() as f64).ceil() as usize - 1];
            println!(
                "{}",
                json!({"trial":trial, "path":if shared {"shared"} else {"full_clone_reference"},
                    "samples":durations.len(), "unit":"us", "p50":percentile(0.5),
                    "p95":percentile(0.95), "p99":percentile(0.99), "p999":percentile(0.999),
                    "max":durations.last().unwrap()})
            );
        }
    }
}
