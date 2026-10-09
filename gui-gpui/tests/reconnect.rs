use futures_util::{SinkExt, StreamExt};
use slimevr_gpui::{
    client::{Client, Connection},
    log_level::LogLevel,
    protocol::Command,
};
use solarxr_protocol::{self as sx, flatbuffers as fb, rpc};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

fn wait(client: &Client, predicate: impl Fn(&slimevr_gpui::client::Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let snapshot = client.snapshot();
        if predicate(&snapshot) {
            return;
        }
        assert!(Instant::now() < deadline, "Timed out: {snapshot:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn disconnect_cancels_unconfirmed_mutation_and_reconnect_restores_reads_only() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            for session in 0..2 {
                let (stream, _) = tokio::time::timeout(Duration::from_secs(6), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let mut subscribed = false;
                let mut reads = 0;
                let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
                loop {
                    let message = tokio::time::timeout_at(deadline, ws.next())
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap();
                    if let Message::Binary(bytes) = message {
                        let bundle = fb::root::<sx::MessageBundle<'_>>(&bytes).unwrap();
                        if let Some(headers) = bundle.data_feed_msgs() {
                            assert!(headers.get(0).message_as_start_data_feed().is_some());
                            subscribed = true;
                        }
                        if let Some(headers) = bundle.rpc_msgs() {
                            for header in headers {
                                match header.message_type() {
                                    rpc::RpcMessage::TrackingPauseStateRequest => {
                                        let mut f = fb::FlatBufferBuilder::new();
                                        let pause = rpc::TrackingPauseStateResponse::create(
                                            &mut f,
                                            &rpc::TrackingPauseStateResponseArgs {
                                                trackingPaused: false,
                                            },
                                        );
                                        let h = rpc::RpcMessageHeader::create(
                                            &mut f,
                                            &rpc::RpcMessageHeaderArgs {
                                                message_type:
                                                    rpc::RpcMessage::TrackingPauseStateResponse,
                                                message: Some(pause.as_union_value()),
                                                ..Default::default()
                                            },
                                        );
                                        let h = f.create_vector(&[h]);
                                        let out = sx::MessageBundle::create(
                                            &mut f,
                                            &sx::MessageBundleArgs {
                                                rpc_msgs: Some(h),
                                                ..Default::default()
                                            },
                                        );
                                        f.finish(out, None);
                                        ws.send(Message::Binary(f.finished_data().to_vec().into()))
                                            .await
                                            .unwrap();
                                        reads += 1;
                                    }
                                    rpc::RpcMessage::SettingsRequest
                                    | rpc::RpcMessage::VRCConfigStateRequest => reads += 1,
                                    rpc::RpcMessage::HeartbeatRequest => (),
                                    rpc::RpcMessage::SetPauseTrackingRequest => {
                                        assert_eq!(
                                            session, 0,
                                            "Mutation was replayed after reconnect"
                                        );
                                        ws.close(None).await.unwrap();
                                        break;
                                    }
                                    rpc::RpcMessage::SkeletonConfigRequest
                                    | rpc::RpcMessage::SerialDevicesRequest
                                    | rpc::RpcMessage::RecordBVHStatusRequest
                                    | rpc::RpcMessage::TrackingChecklistRequest
                                    | rpc::RpcMessage::KeybindRequest
                                    | rpc::RpcMessage::OverlayDisplayModeRequest
                                    | rpc::RpcMessage::ServerInfosRequest
                                    | rpc::RpcMessage::InstalledInfoRequest
                                    | rpc::RpcMessage::MagToggleRequest => (),
                                    other => panic!("Unexpected request: {other:?}"),
                                }
                            }
                            if session == 0
                                && bundle.rpc_msgs().unwrap().iter().any(|h| {
                                    h.message_type() == rpc::RpcMessage::SetPauseTrackingRequest
                                })
                            {
                                break;
                            }
                        }
                        if session == 1
                            && subscribed
                            && bundle.pub_sub_msgs().is_some_and(|h| {
                                h.get(0).u_type() == sx::pub_sub::PubSubUnion::Message
                            })
                            && reads == 3
                        {
                            // Keep the connection open long enough to catch any replayed mutation.
                            while let Ok(Some(Ok(message))) =
                                tokio::time::timeout(Duration::from_millis(400), ws.next()).await
                            {
                                if let Message::Binary(bytes) = message {
                                    let bundle = fb::root::<sx::MessageBundle<'_>>(&bytes).unwrap();
                                    for header in bundle.rpc_msgs().into_iter().flatten() {
                                        assert_eq!(
                                            header.message_type(),
                                            rpc::RpcMessage::HeartbeatRequest
                                        );
                                    }
                                }
                            }
                            return;
                        }
                    }
                }
            }
        });
    });
    let client = Client::connect(format!("ws://{address}"), LogLevel::Error).unwrap();
    wait(&client, |s| {
        s.connection == Connection::Connected && s.paused == Some(false)
    });
    client.send(Command::Pause(true)).unwrap();
    wait(&client, |s| {
        s.session == 2 && s.connection == Connection::Connected
    });
    assert!(client.snapshot().pending.is_none());
    assert_ne!(client.snapshot().paused, Some(true));
    server.join().unwrap();
    let before = Instant::now();
    drop(client);
    assert!(before.elapsed() < Duration::from_secs(3));
}

#[test]
fn disconnected_client_rejects_commands_and_shutdown_cancels_connect_attempt() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = Client::connect(format!("ws://{address}"), LogLevel::Error).unwrap();
    assert!(client.send(Command::Pause(true)).is_err());
    // A TCP-only listener exercises prompt cancellation during the WebSocket upgrade.
    std::thread::sleep(Duration::from_millis(30));
    let before = Instant::now();
    drop(client);
    assert!(before.elapsed() < Duration::from_secs(1));
}
