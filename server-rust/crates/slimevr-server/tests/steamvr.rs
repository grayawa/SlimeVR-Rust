use prost::Message;
use slimevr_core::{
    pose::{PoseEngine, SceneInput},
    skeleton::{BodyPosition as B, HeadPose},
    Quaternion as Q, Vector3 as V,
};
use slimevr_server::{
    api::FrontendConfig,
    receiver::{Receiver, ReceiverConfig},
    steamvr::{
        self, envelope,
        messages::{self, protobuf_message::Message as M},
        Event, Session,
    },
};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::time::Duration;
use tokio::{io::AsyncWriteExt, sync::mpsc};

fn position(id: i32, y: f32) -> messages::ProtobufMessage {
    envelope(M::Position(messages::Position {
        tracker_id: id,
        x: Some(0.0),
        y: Some(y),
        z: Some(0.0),
        qw: 1.0,
        ..Default::default()
    }))
}
fn added(id: i32, role: i32) -> messages::ProtobufMessage {
    envelope(M::TrackerAdded(messages::TrackerAdded {
        tracker_id: id,
        tracker_role: role,
        tracker_serial: format!("OpenVR/{id}"),
        ..Default::default()
    }))
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/steamvr-java-golden.json")).unwrap()
}
#[cfg(unix)]
fn java_sample(name: &str) -> Vec<u8> {
    let data = fixture();
    let hex = data["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap()["hex"]
        .as_str()
        .unwrap();
    slimevr_server::recording::decode_hex(hex).unwrap()
}

#[test]
fn schema_matches_original_java_descriptor_and_actual_java_wire_bytes() {
    fn normalize(message: &mut prost_types::DescriptorProto) {
        for field in &mut message.field {
            field.json_name = None;
        }
        for nested in &mut message.nested_type {
            normalize(nested);
        }
    }
    let mut java = prost_types::FileDescriptorProto::decode(
        include_bytes!("fixtures/steamvr-java-descriptor.pb").as_slice(),
    )
    .unwrap();
    let set = prost_types::FileDescriptorSet::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/steamvr-descriptor.pb")).as_slice(),
    )
    .unwrap();
    let mut driver = set.file.into_iter().next().unwrap();
    for file in [&mut java, &mut driver] {
        file.source_code_info = None;
        for message in &mut file.message_type {
            normalize(message);
        }
    }
    assert_eq!(
        driver, java,
        "driver schema must match the actual server Java message definitions"
    );
    for sample in fixture()["samples"].as_array().unwrap() {
        let bytes = slimevr_server::recording::decode_hex(sample["hex"].as_str().unwrap()).unwrap();
        let decoded = messages::ProtobufMessage::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.encode_to_vec(), bytes, "{}", sample["name"]);
        let framed = steamvr::transport::encode(&decoded).unwrap();
        assert_eq!(
            u32::from_le_bytes(framed[..4].try_into().unwrap()) as usize,
            bytes.len() + 4
        );
        assert_eq!(&framed[4..], bytes);
    }
}

#[tokio::test]
async fn framing_handles_fragmented_and_coalesced_messages_and_rejects_bad_lengths() {
    let bytes: Vec<_> = fixture()["samples"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|sample| {
            let decoded = messages::ProtobufMessage::decode(
                slimevr_server::recording::decode_hex(sample["hex"].as_str().unwrap())
                    .unwrap()
                    .as_slice(),
            )
            .unwrap();
            steamvr::transport::encode(&decoded).unwrap()
        })
        .collect();
    let (mut writer, mut reader) = tokio::io::duplex(16);
    let send = tokio::spawn(async move {
        for byte in bytes {
            writer.write_all(&[byte]).await.unwrap();
        }
    });
    for sample in fixture()["samples"].as_array().unwrap() {
        let message = steamvr::transport::read(&mut reader).await.unwrap();
        assert_eq!(
            message.encode_to_vec(),
            slimevr_server::recording::decode_hex(sample["hex"].as_str().unwrap()).unwrap()
        );
    }
    send.await.unwrap();
    for size in [0u32, 1, 3, 1025, u32::MAX] {
        assert!(steamvr::transport::read(&mut size.to_le_bytes().as_slice())
            .await
            .is_err());
    }
    let huge = envelope(M::TrackerAdded(messages::TrackerAdded {
        tracker_name: "a".repeat(2048),
        ..Default::default()
    }));
    assert!(steamvr::transport::encode(&huge).is_err());
    let mut truncated = steamvr::transport::encode(&position(0, 1.7)).unwrap();
    truncated.pop();
    assert!(steamvr::transport::read(&mut truncated.as_slice())
        .await
        .is_err());
}

#[test]
fn input_status_rotation_only_expiry_and_reconnect_do_not_reuse_stale_poses() {
    let mut session = Session::default();
    session.receive(Event::Connected(1), 0).unwrap();
    session.receive(Event::Message(1, added(0, 19)), 0).unwrap();
    session.receive(Event::Message(1, added(1, 13)), 0).unwrap();
    assert!(matches!(
        session
            .receive(Event::Message(1, position(0, 1.7)), 1)
            .unwrap()[0],
        SceneInput::Head {
            position: Some(_),
            ..
        }
    ));
    assert!(matches!(
        session
            .receive(Event::Message(1, position(1, 1.1)), 1)
            .unwrap()[0],
        SceneInput::Controller {
            body: B::LeftHand,
            ..
        }
    ));
    let mut bad = position(0, 1.7);
    if let Some(M::Position(p)) = &mut bad.message {
        p.qw = f32::NAN;
    }
    assert!(session.receive(Event::Message(1, bad), 2).is_err());
    session
        .receive(
            Event::Message(
                1,
                envelope(M::TrackerStatus(messages::TrackerStatus {
                    tracker_id: 0,
                    status: 4,
                    ..Default::default()
                })),
            ),
            3,
        )
        .unwrap();
    assert!(matches!(
        session
            .receive(Event::Message(1, position(0, 1.7)), 4)
            .unwrap()[0],
        SceneInput::Head { position: None, .. }
    ));
    let clear = session.expire(505);
    assert_eq!(clear.len(), 2);
    assert!(session.expire(506).is_empty());
    session.receive(Event::Disconnected(1), 507).unwrap();
    assert!(!session.status.connected);
    session.receive(Event::Connected(2), 508).unwrap();
    assert!(session
        .receive(Event::Message(1, position(0, 1.7)), 509)
        .unwrap()
        .is_empty());
    assert!(session
        .receive(Event::Message(2, position(0, 1.7)), 509)
        .unwrap()
        .is_empty());
    session
        .receive(Event::Message(2, added(0, 19)), 510)
        .unwrap();
    assert_eq!(
        session
            .receive(Event::Message(2, position(0, 1.7)), 511)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn hand_tracking_controller_handover_keeps_current_sources_in_solarxr_feed() {
    use slimevr_server::api::{protocol, Service};
    use tokio::sync::broadcast;

    // Separate OpenVR IDs can represent the same hand. Late notifications from
    // the old pair must not remove the pair that has already taken over.
    for (hand_roles, controller_roles) in [([11, 12], [13, 14]), ([13, 14], [11, 12])] {
        for retired_event in ["disconnect", "expire", "metadata", "rotation_only"] {
            let config = FrontendConfig::default();
            let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
            let (tx, _) = mpsc::channel(32);
            let (events, _) = broadcast::channel(32);
            let mut service = Service::new(config, None, tx, events);
            let mut session = Session::default();
            session.receive(Event::Connected(1), 0).unwrap();
            for (id, role) in [
                (1, hand_roles[0]),
                (2, hand_roles[1]),
                (3, controller_roles[0]),
                (4, controller_roles[1]),
            ] {
                session
                    .receive(Event::Message(1, added(id, role)), 0)
                    .unwrap();
            }
            for (id, at, height) in [(1, 10, 1.0), (2, 10, 1.0), (3, 500, 1.2), (4, 500, 1.2)] {
                for input in session
                    .receive(Event::Message(1, position(id, height)), at)
                    .unwrap()
                {
                    service.steamvr_input(input, &mut engine).unwrap();
                }
            }
            let retired = match retired_event {
                "expire" => session.expire(511),
                "metadata" => session
                    .receive(Event::Message(1, added(1, hand_roles[0])), 511)
                    .unwrap(),
                "rotation_only" => {
                    let mut p = position(1, 1.0);
                    if let Some(M::Position(p)) = &mut p.message {
                        p.data_source = Some(1);
                    }
                    session.receive(Event::Message(1, p), 511).unwrap()
                }
                _ => session
                    .receive(
                        Event::Message(
                            1,
                            envelope(M::TrackerStatus(messages::TrackerStatus {
                                tracker_id: 1,
                                status: 0,
                                ..Default::default()
                            })),
                        ),
                        511,
                    )
                    .unwrap(),
            };
            for input in retired {
                service.steamvr_input(input, &mut engine).unwrap();
            }
            for body in [B::LeftHand, B::RightHand] {
                assert_eq!(
                    service.external.get(&body).and_then(|p| p.position),
                    Some(V::new(0., 1.2, 0.)),
                    "{hand_roles:?} / {retired_event} cleared {body:?}"
                );
            }
            // Verify the actual list sent to both frontends, not only the pose.
            engine.tick(511).unwrap();
            let frame = protocol::data_frame(
                &protocol::Feed {
                    devices: true,
                    trackers: true,
                    ..Default::default()
                },
                &BTreeMap::new(),
                &BTreeMap::new(),
                engine.snapshot(),
                &service.config.pose,
                &BTreeMap::new(),
                &service.external,
            );
            let bundle =
                solarxr_protocol::flatbuffers::root::<solarxr_protocol::MessageBundle>(&frame)
                    .unwrap();
            let feed = bundle
                .data_feed_msgs()
                .unwrap()
                .get(0)
                .message_as_data_feed_update()
                .unwrap();
            let ids: Vec<_> = feed
                .devices()
                .unwrap()
                .get(0)
                .trackers()
                .unwrap()
                .iter()
                .map(|t| t.tracker_id().unwrap().tracker_num())
                .collect();
            assert_eq!(
                ids,
                [1, 2],
                "{retired_event} removed a hand from the UI feed"
            );
            // Losing the current pair still clears it; a future pose restores it.
            for id in [3, 4] {
                for input in session
                    .receive(
                        Event::Message(
                            1,
                            envelope(M::TrackerStatus(messages::TrackerStatus {
                                tracker_id: id,
                                status: 0,
                                ..Default::default()
                            })),
                        ),
                        512,
                    )
                    .unwrap()
                {
                    service.steamvr_input(input, &mut engine).unwrap();
                }
            }
            assert!(service.external.is_empty());
            // A disconnected pair can resume without another registration.
            for id in [3, 4] {
                for message in [
                    envelope(M::TrackerStatus(messages::TrackerStatus {
                        tracker_id: id,
                        status: 1,
                        ..Default::default()
                    })),
                    position(id, 1.3),
                ] {
                    for input in session.receive(Event::Message(1, message), 513).unwrap() {
                        service.steamvr_input(input, &mut engine).unwrap();
                    }
                }
            }
            assert_eq!(service.external.len(), 2);
        }
    }
}

#[tokio::test]
async fn output_uses_final_computed_pose_stable_serials_and_retries_registration_under_backpressure(
) {
    let mut config = FrontendConfig::default();
    config.steam_vr.automatic = false;
    for (_, role, _) in steamvr::ROLES {
        config.steam_vr.trackers.insert(role.into(), true);
    }
    let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
    engine
        .set_head(
            0,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.0, 1.7, 0.0)),
            },
        )
        .unwrap();
    engine.tick(0).unwrap();
    let mut pose = engine.snapshot().clone();
    pose.skeleton.computed.get_mut("hip").unwrap().position = V::new(0.11, 0.22, 0.33);
    let receiver = Receiver::new(ReceiverConfig::default()).unwrap();
    let mut session = Session::default();
    session.receive(Event::Connected(1), 0).unwrap();
    let (sender, mut output) = mpsc::channel(1);
    session.output(&config, &pose, &receiver, &sender);
    // Occupied queue: new share changes must not be acknowledged/lost.
    config.steam_vr.trackers.insert("head".into(), false);
    session.output(&config, &pose, &receiver, &sender);
    let stats = session.output_stats().clone();
    assert_eq!(stats.snapshot().steamvr_output_batches_enqueued, 1);
    assert_eq!(stats.snapshot().steamvr_output_queue_full, 1);
    assert_eq!(stats.snapshot().steamvr_output_batches_written, 0);
    let first = output.recv().await.unwrap();
    assert_eq!(
        first
            .messages
            .iter()
            .filter(|m| matches!(m.message, Some(M::TrackerAdded(_))))
            .count(),
        11
    );
    session
        .receive(
            Event::Message(
                1,
                envelope(M::Version(messages::Version {
                    protocol_version: 2,
                })),
            ),
            1,
        )
        .unwrap();
    session.output(&config, &pose, &receiver, &sender);
    let batch = output.recv().await.unwrap();
    assert!(batch.messages.iter().any(
        |m| matches!(&m.message,Some(M::TrackerStatus(s)) if s.tracker_id==15 && s.status==0)
    ));
    let hip = batch
        .messages
        .iter()
        .find_map(|m| {
            if let Some(M::Position(p)) = &m.message {
                (p.tracker_id == 1).then_some(p)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!((hip.x, hip.y, hip.z), (Some(0.11), Some(0.22), Some(0.33)));
    session.receive(Event::Disconnected(1), 2).unwrap();
    session.receive(Event::Connected(2), 3).unwrap();
    session.output(&config, &pose, &receiver, &sender);
    let reconnected = output.recv().await.unwrap();
    assert_eq!(reconnected.session, 2);
    for message in reconnected.messages {
        if let Some(M::TrackerAdded(t)) = message.message {
            assert!(t.tracker_serial.starts_with("human://"));
            assert_eq!(t.tracker_id, t.tracker_role);
        }
    }
    assert_eq!(stats.snapshot().steamvr_output_batches_enqueued, 3);
    drop(output);
    config.steam_vr.trackers.insert("head".into(), true);
    session.output(&config, &pose, &receiver, &sender);
    assert_eq!(stats.snapshot().steamvr_output_queue_closed, 1);
}

#[test]
fn automatic_share_maps_assigned_roles_preserves_hand_choices_and_yaml_extensions() {
    let mut config=slimevr_server::config::from_yaml(serde_yaml_ng::from_str("bridges:\n  steamvr:\n    unknownOption: keep\n    trackers: {left_hand: true, future_role: true}\n").unwrap()).unwrap();
    let mut pose = PoseEngine::new(config.pose.clone()).unwrap();
    pose.tick(0).unwrap();
    assert!(!config.steam_vr.update_automatic(pose.snapshot()));
    config
        .pose
        .bindings
        .push(slimevr_core::pose::TrackerBinding {
            device_key: "test".into(),
            sensor_id: 0,
            body: B::Chest,
            mounting: Q::IDENTITY,
        });
    let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
    engine.tick(0).unwrap();
    assert!(config.steam_vr.update_automatic(engine.snapshot()));
    assert!(
        config.steam_vr.enabled("waist")
            && config.steam_vr.enabled("chest")
            && config.steam_vr.enabled("left_hand")
    );
    let yaml = slimevr_server::config::to_yaml(&config).unwrap();
    assert_eq!(
        yaml["bridges"]["steamvr"]["unknownOption"].as_str(),
        Some("keep")
    );
    let reloaded = slimevr_server::config::from_yaml(yaml).unwrap();
    assert!(reloaded.steam_vr.enabled("future_role"));
    let mut settings = steamvr::Settings {
        automatic: false,
        trackers: BTreeMap::new(),
    };
    assert!(!settings.update_automatic(engine.snapshot()));
}

#[tokio::test]
async fn steamvr_emits_optional_derived_velocity_and_clears_it_on_disable_or_invalid_pose() {
    let mut config = FrontendConfig::default();
    config.pose.send_derived_velocity = true;
    config.pose.legs.enabled = false;
    config.steam_vr.trackers.insert("waist".into(), true);
    let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
    engine
        .set_head(
            0,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.0, 1.7, 0.0)),
            },
        )
        .unwrap();
    let first = engine.tick(0).unwrap().clone();
    engine
        .set_head(
            20,
            HeadPose {
                rotation: Q::IDENTITY,
                position: Some(V::new(0.02, 1.7, -0.04)),
            },
        )
        .unwrap();
    let moving = engine.tick(20).unwrap().clone();
    let mut invalid = moving.clone();
    invalid.skeleton.world_anchor_present = false;
    let receiver = Receiver::new(ReceiverConfig::default()).unwrap();
    let mut session = Session::default();
    session.receive(Event::Connected(1), 0).unwrap();
    let (sender, mut output) = mpsc::channel(4);
    for (pose, enabled, expected) in [
        (&first, true, None),
        (&moving, true, Some(moving.computed_velocities["hip"])),
        (&moving, false, None),
    ] {
        config.pose.send_derived_velocity = enabled;
        session
            .receive(
                Event::Message(
                    1,
                    envelope(M::Version(messages::Version {
                        protocol_version: 2,
                    })),
                ),
                pose.at_ms,
            )
            .unwrap();
        session.output(&config, pose, &receiver, &sender);
        let batch = output.recv().await.unwrap();
        let message = batch
            .messages
            .iter()
            .find_map(|m| match &m.message {
                Some(M::Position(p)) if p.tracker_id == 1 => Some(p),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            (message.vx, message.vy, message.vz),
            (
                expected.map(|v| v.x),
                expected.map(|v| v.y),
                expected.map(|v| v.z)
            )
        );
        let bytes = message.encode_to_vec();
        assert_eq!(
            messages::Position::decode(bytes.as_slice()).unwrap(),
            *message
        );
    }
    config.pose.send_derived_velocity = true;
    session
        .receive(
            Event::Message(
                1,
                envelope(M::Version(messages::Version {
                    protocol_version: 2,
                })),
            ),
            21,
        )
        .unwrap();
    session.output(&config, &invalid, &receiver, &sender);
    let batch = output.recv().await.unwrap();
    assert!(!batch
        .messages
        .iter()
        .any(|m| matches!(m.message, Some(M::Position(_)))));
    assert!(batch
        .messages
        .iter()
        .any(|m| matches!(&m.message, Some(M::TrackerStatus(s)) if s.status == 3)));
}

#[cfg(unix)]
#[tokio::test]
async fn socket_ownership_stale_cleanup_and_stream_reconnect_are_safe() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("SlimeVRDriver");
    std::fs::write(&path, b"keep").unwrap();
    assert!(steamvr::transport::Listener::bind(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    std::fs::remove_file(&path).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    let listener = steamvr::transport::Listener::bind(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(steamvr::transport::Listener::bind(&path).is_err());
    drop(listener);
    let listener = steamvr::transport::Listener::bind(&path).unwrap();
    let (sender, mut events) = mpsc::channel(16);
    let (output, receiver) = mpsc::channel(4);
    let stats = std::sync::Arc::new(steamvr::OutputStats::default());
    let task = tokio::spawn(listener.run_with_stats(sender, receiver, stats.clone()));
    let mut client = tokio::net::UnixStream::connect(&path).await.unwrap();
    // The ownership probe above may enqueue an empty connection; this fresh listener has none.
    let session = match tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .unwrap()
        .unwrap()
    {
        Event::Connected(s) => s,
        _ => panic!(),
    };
    client
        .write_all(&steamvr::transport::encode(&position(0, 1.7)).unwrap())
        .await
        .unwrap();
    assert!(matches!(events.recv().await,Some(Event::Message(s,_)) if s==session));
    drop(client);
    assert!(matches!(events.recv().await,Some(Event::Disconnected(s)) if s==session));
    let mut client = tokio::net::UnixStream::connect(&path).await.unwrap();
    let next = match events.recv().await.unwrap() {
        Event::Connected(s) => s,
        _ => panic!(),
    };
    assert_ne!(session, next);
    output
        .send(steamvr::Batch {
            session,
            messages: vec![position(0, 2.0)],
        })
        .await
        .unwrap();
    output
        .send(steamvr::Batch {
            session: next,
            messages: vec![position(0, 1.7)],
        })
        .await
        .unwrap();
    assert_eq!(
        steamvr::transport::read(&mut client).await.unwrap(),
        position(0, 1.7)
    );
    // Finish notification can run just after the reader observes the final bytes.
    tokio::time::timeout(Duration::from_secs(1), async {
        while stats.snapshot().steamvr_output_batches_written == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(stats.snapshot().steamvr_output_batches_written, 1);
    assert_eq!(stats.snapshot().steamvr_output_stale_batches, 1);
    task.abort();
    let _ = task.await;
    assert!(!path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn bindings_provider_starts_after_version_and_is_reaped_on_disconnect() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let endpoint = dir.path().join("SlimeVRDriver");
    let marker = dir.path().join("pid");
    let script = dir.path().join("provider");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (commands, _requests) = mpsc::channel(1);
    let mut bridge = steamvr::Bridge::start(
        &endpoint,
        Some(script),
        false,
        commands,
        tokio::sync::broadcast::channel(32).0,
        Default::default(),
    )
    .unwrap();
    bridge.receive(Event::Connected(1), 0).unwrap();
    bridge
        .receive(
            Event::Message(
                1,
                envelope(M::Version(messages::Version {
                    protocol_version: 2,
                })),
            ),
            0,
        )
        .unwrap();
    bridge.provider_tick(0);
    bridge.provider_tick(2999);
    assert!(!marker.exists());
    bridge.provider_tick(3000);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    bridge.provider_tick(3001);
    assert!(bridge.state.status.bindings_provider_running);
    let pid = std::fs::read_to_string(&marker).unwrap().trim().to_owned();
    bridge.receive(Event::Disconnected(1), 3002).unwrap();
    assert!(!bridge.state.status.bindings_provider_running);
    #[cfg(target_os = "linux")]
    {
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        tokio::time::timeout(Duration::from_secs(1), async {
            while proc.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn real_cli_bridges_java_input_native_rpc_shared_roles_reconnect_and_records_replayable_controls(
) {
    use slimevr_server::api::protocol::rpc_frame;
    use solarxr_protocol::{self as sx, flatbuffers as fb, rpc};
    use tokio::io::AsyncReadExt;
    let dir = tempfile::tempdir().unwrap();
    let endpoint = dir.path().join("SlimeVRDriver");
    let config = dir.path().join("vrconfig.yml");
    let journal = dir.path().join("run.jsonl");
    std::fs::write(&config,"version: '15'\nvelocityConfig:\n  sendDerivedVelocity: true\nbridges:\n  steamvr:\n    automaticSharedTrackersToggling: false\n    trackers: {waist: true, left_foot: true, right_foot: true}\n").unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .args([
            "listen",
            "--bind",
            "127.0.0.1:0",
            "--api-bind",
            "127.0.0.1:0",
            "--no-discovery",
            "--no-bindings-provider",
            "--pose-output-ms",
            "20",
            "--log-level",
            "debug",
            "--run-for",
            "3",
            "--config",
        ])
        .arg(&config)
        .arg("--record")
        .arg(&journal)
        .arg("--steamvr-endpoint")
        .arg(&endpoint)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let stdout = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let mut stderr = child.stderr.take().unwrap();
    let stderr = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let run = async {
        let mut driver = loop {
            if let Ok(stream) = tokio::net::UnixStream::connect(&endpoint).await {
                break stream;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let mut registered = BTreeMap::new();
        while registered.len() < 3 {
            if let Some(M::TrackerAdded(t)) =
                steamvr::transport::read(&mut driver).await.unwrap().message
            {
                registered.insert(t.tracker_id, t.tracker_serial);
            }
        }
        for name in [
            "version", "added0", "pose0", "added1", "pose1", "added2", "pose2",
        ] {
            let bytes = java_sample(name);
            driver
                .write_all(&((bytes.len() + 4) as u32).to_le_bytes())
                .await
                .unwrap();
            for fragment in bytes.chunks(3) {
                driver.write_all(fragment).await.unwrap();
            }
        }
        loop {
            if let Some(M::Position(p)) =
                steamvr::transport::read(&mut driver).await.unwrap().message
            {
                if p.tracker_id == 1 {
                    assert!(p.y.unwrap() > 0.0);
                    if let (Some(vx), Some(vy), Some(vz)) = (p.vx, p.vy, p.vz) {
                        assert!(vx.is_finite() && vy.is_finite() && vz.is_finite());
                        break;
                    }
                    let bytes = java_sample("version");
                    driver
                        .write_all(&((bytes.len() + 4) as u32).to_le_bytes())
                        .await
                        .unwrap();
                    driver.write_all(&bytes).await.unwrap();
                }
            }
        }
        let mut native = tokio::net::UnixStream::connect(endpoint.with_file_name("SlimeVRRpc"))
            .await
            .unwrap();
        let frame = rpc_frame(rpc::RpcMessage::SettingsRequest, 42, |f| {
            rpc::SettingsRequest::create(f, &Default::default()).as_union_value()
        });
        native
            .write_all(&((frame.len() + 4) as u32).to_le_bytes())
            .await
            .unwrap();
        native.write_all(&frame).await.unwrap();
        let bytes = steamvr::transport::read_bytes(&mut native, 64 * 1024)
            .await
            .unwrap();
        let bundle = fb::root::<sx::MessageBundle>(&bytes).unwrap();
        let response = bundle.rpc_msgs().unwrap().get(0);
        assert_eq!(response.tx_id().unwrap().id(), 42);
        assert!(response
            .message_as_settings_response()
            .unwrap()
            .steam_vr_trackers()
            .unwrap()
            .waist());
        let frame = rpc_frame(rpc::RpcMessage::ChangeSettingsRequest, 43, |f| {
            let steam = rpc::SteamVRTrackersSetting::create(
                f,
                &rpc::SteamVRTrackersSettingArgs {
                    waist: true,
                    automaticTrackerToggle: false,
                    ..Default::default()
                },
            );
            rpc::ChangeSettingsRequest::create(
                f,
                &rpc::ChangeSettingsRequestArgs {
                    steam_vr_trackers: Some(steam),
                    ..Default::default()
                },
            )
            .as_union_value()
        });
        native
            .write_all(&((frame.len() + 4) as u32).to_le_bytes())
            .await
            .unwrap();
        native.write_all(&frame).await.unwrap();
        loop {
            if let Some(M::TrackerStatus(s)) =
                steamvr::transport::read(&mut driver).await.unwrap().message
            {
                if matches!(s.tracker_id, 2 | 3) && s.status == 0 {
                    break;
                }
            }
        }
        // Preserve at least one published velocity frame before pausing.
        tokio::time::sleep(Duration::from_millis(30)).await;
        for name in ["pause_tracking", "fast_reset"] {
            let bytes = java_sample(name);
            driver
                .write_all(&((bytes.len() + 4) as u32).to_le_bytes())
                .await
                .unwrap();
            driver.write_all(&bytes).await.unwrap();
        }
        drop(driver);
        tokio::time::sleep(Duration::from_millis(30)).await;
        let mut driver = tokio::net::UnixStream::connect(&endpoint).await.unwrap();
        loop {
            if let Some(M::TrackerAdded(t)) =
                steamvr::transport::read(&mut driver).await.unwrap().message
            {
                assert_eq!(registered.get(&t.tracker_id), Some(&t.tracker_serial));
                break;
            }
        }
        drop(driver);
        drop(native);
    };
    tokio::time::timeout(Duration::from_secs(2), run)
        .await
        .expect("SteamVR bridge integration timed out");
    let status = child.wait().await.unwrap();
    let stderr = stderr.await.unwrap();
    let output = stdout.await.unwrap();
    assert!(status.success(), "{}", String::from_utf8_lossy(&stderr));
    assert!(!endpoint.exists());
    assert!(!endpoint.with_file_name("SlimeVRRpc").exists());
    let config = FrontendConfig::load(&config).unwrap();
    assert!(config.steam_vr.enabled("waist"));
    assert!(!config.steam_vr.enabled("left_foot"));
    let poses: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .filter(|v: &serde_json::Value| v["type"] == "pose_snapshot")
        .collect();
    assert!(poses.last().unwrap()["paused"].as_bool().unwrap());
    assert!(poses.iter().any(|pose| pose["computed_velocities"]
        .as_object()
        .is_some_and(|v| !v.is_empty())));
    let replay = slimevr_server::recording::replay(&journal, |_| {}).unwrap();
    assert!(replay.at_ms >= 2900);
    let replay = std::process::Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .arg("solve-recording")
        .arg(&journal)
        .arg("--config")
        .arg(dir.path().join("vrconfig.yml"))
        .arg("--frames")
        .output()
        .unwrap();
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let replay_poses: BTreeMap<u64, serde_json::Value> = String::from_utf8(replay.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|pose| pose["type"] == "pose_snapshot")
        .map(|pose| (pose["frame"].as_u64().unwrap(), pose))
        .collect();
    for mut pose in poses {
        pose.as_object_mut().unwrap().remove("level");
        assert_eq!(
            replay_poses.get(&pose["frame"].as_u64().unwrap()),
            Some(&pose),
            "live and replayed velocities must agree"
        );
    }
}
