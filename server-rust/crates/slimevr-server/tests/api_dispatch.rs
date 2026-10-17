//! Observable batch semantics across configuration, tracking, hardware and status RPCs.
use slimevr_core::pose::PoseEngine;
use slimevr_server::{
    api::{FrontendConfig, Service, Wire},
    receiver::{Receiver, ReceiverConfig},
};
use solarxr_protocol::{self as sx, datatypes as dt, flatbuffers as fb, rpc};
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy)]
enum Command {
    Query(rpc::RpcMessage),
    Approve,
    Width(f32),
    Pause(bool),
    Unsupported,
    Aligned(bool),
    ClearPose(rpc::StayAlignedRelaxedPose),
    DetectPose(rpc::StayAlignedRelaxedPose),
}

fn batch(commands: &[Command]) -> Message {
    let mut f = fb::FlatBufferBuilder::new();
    let mut headers = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let (kind, payload) = match *command {
            Command::Aligned(enable) => (
                rpc::RpcMessage::EnableStayAlignedRequest,
                Some(
                    rpc::EnableStayAlignedRequest::create(
                        &mut f,
                        &rpc::EnableStayAlignedRequestArgs { enable },
                    )
                    .as_union_value(),
                ),
            ),
            Command::DetectPose(pose) => (
                rpc::RpcMessage::DetectStayAlignedRelaxedPoseRequest,
                Some(
                    rpc::DetectStayAlignedRelaxedPoseRequest::create(
                        &mut f,
                        &rpc::DetectStayAlignedRelaxedPoseRequestArgs { pose },
                    )
                    .as_union_value(),
                ),
            ),
            Command::ClearPose(pose) => (
                rpc::RpcMessage::ResetStayAlignedRelaxedPoseRequest,
                Some(
                    rpc::ResetStayAlignedRelaxedPoseRequest::create(
                        &mut f,
                        &rpc::ResetStayAlignedRelaxedPoseRequestArgs { pose },
                    )
                    .as_union_value(),
                ),
            ),
            Command::Approve => {
                let mac = f.create_string("02:00:00:00:00:01");
                (
                    rpc::RpcMessage::AddUnknownDeviceRequest,
                    Some(
                        rpc::AddUnknownDeviceRequest::create(
                            &mut f,
                            &rpc::AddUnknownDeviceRequestArgs {
                                mac_address: Some(mac),
                            },
                        )
                        .as_union_value(),
                    ),
                )
            }
            Command::Width(value) => (
                rpc::RpcMessage::ChangeSkeletonConfigRequest,
                Some(
                    rpc::ChangeSkeletonConfigRequest::create(
                        &mut f,
                        &rpc::ChangeSkeletonConfigRequestArgs {
                            bone: rpc::SkeletonBone::HIPS_WIDTH,
                            value,
                        },
                    )
                    .as_union_value(),
                ),
            ),
            Command::Pause(pause_tracking) => (
                rpc::RpcMessage::SetPauseTrackingRequest,
                Some(
                    rpc::SetPauseTrackingRequest::create(
                        &mut f,
                        &rpc::SetPauseTrackingRequestArgs {
                            pauseTracking: pause_tracking,
                        },
                    )
                    .as_union_value(),
                ),
            ),
            Command::Unsupported => (rpc::RpcMessage::NONE, None),
            Command::Query(kind) => {
                let payload = match kind {
                    rpc::RpcMessage::HeartbeatRequest => {
                        rpc::HeartbeatRequest::create(&mut f, &Default::default()).as_union_value()
                    }
                    rpc::RpcMessage::SettingsRequest => {
                        rpc::SettingsRequest::create(&mut f, &Default::default()).as_union_value()
                    }
                    rpc::RpcMessage::SkeletonConfigRequest => {
                        rpc::SkeletonConfigRequest::create(&mut f, &Default::default())
                            .as_union_value()
                    }
                    rpc::RpcMessage::TrackingPauseStateRequest => {
                        rpc::TrackingPauseStateRequest::create(&mut f, &Default::default())
                            .as_union_value()
                    }
                    rpc::RpcMessage::MagToggleRequest => {
                        rpc::MagToggleRequest::create(&mut f, &Default::default()).as_union_value()
                    }
                    rpc::RpcMessage::SerialDevicesRequest => {
                        rpc::SerialDevicesRequest::create(&mut f, &Default::default())
                            .as_union_value()
                    }
                    rpc::RpcMessage::RecordBVHStatusRequest => {
                        rpc::RecordBVHStatusRequest::create(&mut f, &Default::default())
                            .as_union_value()
                    }
                    _ => panic!("unsupported test query"),
                };
                (kind, Some(payload))
            }
        };
        let tx = dt::TransactionId::new(index as u32 + 1);
        headers.push(rpc::RpcMessageHeader::create(
            &mut f,
            &rpc::RpcMessageHeaderArgs {
                tx_id: Some(&tx),
                message_type: kind,
                message: payload,
            },
        ));
    }
    let headers = f.create_vector(&headers);
    let bundle = sx::MessageBundle::create(
        &mut f,
        &sx::MessageBundleArgs {
            rpc_msgs: Some(headers),
            ..Default::default()
        },
    );
    f.finish(bundle, None);
    Message::Binary(f.finished_data().to_vec().into())
}

fn setup(path: Option<PathBuf>) -> (Service, Receiver, PoseEngine, broadcast::Receiver<Wire>) {
    let config = FrontendConfig::default();
    let engine = PoseEngine::new(config.pose.clone()).unwrap();
    let receiver = Receiver::new(ReceiverConfig::default()).unwrap();
    let (commands, _) = mpsc::channel(32);
    let (events, updates) = broadcast::channel(32);
    (
        Service::new(config, path, commands, events),
        receiver,
        engine,
        updates,
    )
}

fn identity(wire: &Wire) -> (rpc::RpcMessage, u32) {
    let Wire::Binary(bytes) = wire else {
        panic!("expected binary response")
    };
    let bundle = fb::root::<sx::MessageBundle>(bytes).unwrap();
    let header = bundle.rpc_msgs().unwrap().get(0);
    (header.message_type(), header.tx_id().unwrap().id())
}

#[tokio::test]
async fn mixed_batch_preserves_reply_order_transactions_broadcasts_and_yaml() {
    use rpc::RpcMessage as R;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yml");
    let (mut service, mut receiver, mut engine, mut events) = setup(Some(path.clone()));
    let responses = service.handle(
        batch(&[
            Command::Query(R::HeartbeatRequest),
            Command::Approve,
            Command::Width(0.36),
            Command::Pause(true),
            Command::Query(R::TrackingPauseStateRequest),
            Command::Query(R::MagToggleRequest),
            Command::Query(R::SerialDevicesRequest),
            Command::Query(R::RecordBVHStatusRequest),
            Command::Query(R::SkeletonConfigRequest),
            Command::Query(R::SettingsRequest),
        ]),
        &mut receiver,
        &mut engine,
        10,
    );
    assert_eq!(
        responses.iter().map(identity).collect::<Vec<_>>(),
        [
            (R::HeartbeatResponse, 1),
            (R::TrackingPauseStateResponse, 5),
            (R::MagToggleResponse, 6),
            (R::SerialDevicesResponse, 7),
            (R::RecordBVHStatus, 8),
            (R::SkeletonConfigResponse, 9),
            (R::SettingsResponse, 10),
        ]
    );
    assert!(engine.is_paused());
    assert_eq!(engine.export_config().skeleton.hips_width, 0.36);
    service.finish_config_save().await.unwrap();
    let saved = FrontendConfig::load(&path).unwrap();
    assert_eq!(saved.pose.skeleton.hips_width, 0.36);
    assert_eq!(saved.allowed_macs, ["02:00:00:00:00:01"]);
    assert_eq!(receiver.config.allowed_macs, saved.allowed_macs);
    let mut broadcasts = Vec::new();
    while let Ok(event) = events.try_recv() {
        broadcasts.push(identity(&event));
    }
    assert_eq!(
        broadcasts,
        [
            (R::SettingsResponse, 0),
            (R::SkeletonConfigResponse, 0),
            (R::TrackingPauseStateResponse, 4),
        ]
    );
}

#[test]
fn failed_batch_stops_later_commands_keeps_prior_changes_and_returns_settings() {
    let (mut service, mut receiver, mut engine, _) = setup(None);
    let responses = service.handle(
        batch(&[
            Command::Approve,
            Command::Pause(true),
            Command::Unsupported,
            Command::Pause(false),
        ]),
        &mut receiver,
        &mut engine,
        10,
    );
    assert!(
        engine.is_paused(),
        "later command must not execute after a failure"
    );
    assert_eq!(receiver.config.allowed_macs, ["02:00:00:00:00:01"]);
    assert_eq!(responses.len(), 2);
    let Wire::Text(error) = &responses[0] else {
        panic!("expected error")
    };
    let error: serde_json::Value = serde_json::from_str(error).unwrap();
    assert_eq!(error["type"], "backend_error");
    assert!(error["message"].as_str().unwrap().contains("not available"));
    assert_eq!(
        identity(&responses[1]),
        (rpc::RpcMessage::SettingsResponse, 0)
    );
}

#[test]
fn oversized_batch_is_rejected_before_any_side_effect() {
    let (mut service, mut receiver, mut engine, _) = setup(None);
    let mut commands = vec![Command::Query(rpc::RpcMessage::HeartbeatRequest); 33];
    commands[0] = Command::Approve;
    let responses = service.handle(batch(&commands), &mut receiver, &mut engine, 10);
    assert!(service.config.allowed_macs.is_empty());
    assert!(receiver.config.allowed_macs.is_empty());
    let Wire::Text(error) = &responses[0] else {
        panic!("expected error")
    };
    let error: serde_json::Value = serde_json::from_str(error).unwrap();
    assert_eq!(error["message"], "too many commands");
}

#[tokio::test]
async fn stay_aligned_skip_all_poses_keeps_settings_unlocked_after_disable_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vrconfig.yml");
    let (mut service, mut receiver, mut engine, _) = setup(Some(path.clone()));
    for enable in [true, false] {
        let responses = service.handle(
            batch(&[
                Command::ClearPose(rpc::StayAlignedRelaxedPose::STANDING),
                Command::ClearPose(rpc::StayAlignedRelaxedPose::SITTING),
                Command::ClearPose(rpc::StayAlignedRelaxedPose::FLAT),
                Command::Aligned(enable),
                Command::Query(rpc::RpcMessage::SettingsRequest),
            ]),
            &mut receiver,
            &mut engine,
            10,
        );
        let Wire::Binary(bytes) = responses.last().unwrap() else {
            panic!("expected settings");
        };
        let bundle = fb::root::<sx::MessageBundle>(bytes).unwrap();
        let aligned = bundle
            .rpc_msgs()
            .unwrap()
            .get(0)
            .message_as_settings_response()
            .unwrap()
            .stay_aligned()
            .unwrap();
        assert!(aligned.setupComplete());
        assert_eq!(aligned.enabled(), enable);
        assert!(!aligned.standingEnabled() && !aligned.sittingEnabled() && !aligned.flatEnabled());
    }
    service.finish_config_save().await.unwrap();
    let saved = FrontendConfig::load(&path).unwrap();
    assert!(saved.pose.alignment.setup_complete);
    assert!(!saved.pose.alignment.enabled);
}

#[test]
fn relaxed_pose_capture_uses_selected_tracker_for_each_duplicate_body_assignment() {
    use slimevr_core::{
        pose::TrackerBinding, skeleton::BodyPosition as B, Quaternion as Q, TrackerSample,
    };
    let mut config = FrontendConfig::default();
    config.pose.bindings = [
        ("a-selected", B::LeftUpperLeg),
        ("z-backup", B::LeftUpperLeg),
        ("right", B::RightUpperLeg),
    ]
    .into_iter()
    .map(|(key, body)| TrackerBinding {
        device_key: key.into(),
        sensor_id: 0,
        body,
        mounting: Q::IDENTITY,
    })
    .collect();
    let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
    for (key, q) in [
        ("a-selected", Q::rotation_y(0.4)),
        ("z-backup", Q::rotation_y(-0.8)),
        ("right", Q::IDENTITY),
    ] {
        engine
            .sample(&TrackerSample {
                source: "test".into(),
                device_key: key.into(),
                sensor_id: 0,
                session: 1,
                packet_sequence: 1,
                received_at_ms: 1,
                socket_received_at_ms: None,
                sensor_timestamp_us: None,
                packet_rotation: None,
                server_rotation: Some(q),
                packet_acceleration: None,
                server_acceleration: None,
                position: None,
                compatibility_fallback: false,
            })
            .unwrap();
    }
    engine.tick(2).unwrap();
    assert!(
        engine
            .snapshot()
            .trackers
            .iter()
            .find(|t| t.device_key == "a-selected")
            .unwrap()
            .selected
    );
    let (commands, _) = mpsc::channel(32);
    let (events, _) = broadcast::channel(32);
    let mut service = Service::new(config, None, commands, events);
    let mut receiver = Receiver::new(ReceiverConfig::default()).unwrap();
    service.handle(
        batch(&[Command::DetectPose(rpc::StayAlignedRelaxedPose::STANDING)]),
        &mut receiver,
        &mut engine,
        3,
    );
    let captured = service.config.pose.alignment.standing;
    assert!(captured.enabled);
    assert!(
        (captured.upper_leg_degrees - 0.2f32.to_degrees()).abs() < 0.01,
        "{captured:?}"
    );
}
