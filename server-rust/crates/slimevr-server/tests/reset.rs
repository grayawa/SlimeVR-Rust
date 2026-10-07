use slimevr_core::pose::PoseEngine;
use slimevr_server::{
    api::{self, protocol, FrontendConfig, Service, Wire},
    receiver::{Receiver, ReceiverConfig},
};
use solarxr_protocol::{self as sx, flatbuffers as fb, rpc};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

fn reset_progress(wire: Wire) -> (rpc::ResetStatus, i32, i32) {
    let Wire::Binary(bytes) = wire else {
        panic!("expected binary RPC")
    };
    let bundle = fb::root::<sx::MessageBundle>(&bytes).unwrap();
    let headers = bundle.rpc_msgs().unwrap();
    let header = headers.get(0);
    assert_eq!(header.tx_id().unwrap().id(), 7);
    let response = header.message_as_reset_response().unwrap();
    (response.status(), response.progress(), response.duration())
}

#[test]
fn delayed_reset_reports_whole_seconds_once_and_completes_once() {
    for duration in [3000u64, 2500, 500] {
        let config = FrontendConfig::default();
        let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
        let mut receiver = Receiver::new(ReceiverConfig::default()).unwrap();
        let (commands, _) = mpsc::channel::<api::Request>(32);
        let (events, mut updates) = broadcast::channel(32);
        let mut service = Service::new(config, None, commands, events);
        let bytes = protocol::rpc_frame(rpc::RpcMessage::ResetRequest, 7, |f| {
            rpc::ResetRequest::create(
                f,
                &rpc::ResetRequestArgs {
                    reset_type: rpc::ResetType::Full,
                    delay: Some(duration as f32 / 1000.0),
                    ..Default::default()
                },
            )
            .as_union_value()
        });
        let started = service.handle(
            Message::Binary(bytes.into()),
            &mut receiver,
            &mut engine,
            13,
        );
        assert_eq!(started.len(), 1);
        let mut received: Vec<_> = started.into_iter().map(reset_progress).collect();
        // The pose clock runs much more frequently than the countdown notifications.
        for at in (13..duration + 114).step_by(4) {
            service.before_tick(&mut engine, at);
            while let Ok(event) = updates.try_recv() {
                received.push(reset_progress(event));
            }
        }
        let expected: Vec<_> = (0..duration)
            .step_by(1000)
            .map(|progress| (rpc::ResetStatus::STARTED, progress as i32, duration as i32))
            .chain([(rpc::ResetStatus::FINISHED, duration as i32, duration as i32)])
            .collect();
        assert_eq!(
            received, expected,
            "duplicate countdown notes for delay {duration}"
        );
        let snapshot = engine.tick(duration + 114).unwrap();
        assert_eq!(snapshot.reset_count, 1);
        assert!(snapshot.last_full_reset_ms.is_some());
    }
}
