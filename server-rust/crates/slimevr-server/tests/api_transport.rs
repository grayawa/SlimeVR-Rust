//! Exercise the API lifecycle over real WebSocket connections.
use futures_util::{SinkExt, StreamExt};
use slimevr_core::pose::PoseEngine;
use slimevr_server::{
    api::{self, protocol, pubsub, FrontendConfig, LiveState, Request, Service, Wire},
    receiver::{Receiver, ReceiverConfig},
};
use solarxr_protocol::{self as sx, data_feed as df, flatbuffers as fb, pub_sub as ps, rpc};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc, watch},
    task::JoinHandle,
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct Running {
    address: SocketAddr,
    state: watch::Sender<Arc<LiveState>>,
    server: JoinHandle<()>,
    owner: JoinHandle<()>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.server.abort();
        self.owner.abort();
    }
}
impl Running {
    async fn start() -> Self {
        let config = FrontendConfig::default();
        let mut engine = PoseEngine::new(config.pose.clone()).unwrap();
        let mut receiver = Receiver::new(ReceiverConfig::default()).unwrap();
        let (commands, mut requests) = mpsc::channel(32);
        let (events, _) = broadcast::channel::<Wire>(64);
        let mut service = Service::new(config, None, commands.clone(), events.clone());
        let (state, live) = watch::channel(service.live(&receiver, &engine, 0));
        let (address, server) = api::serve(
            "127.0.0.1:0".parse().unwrap(),
            commands,
            live,
            events,
            pubsub::Hub::default(),
        )
        .await
        .unwrap();
        let owner = tokio::spawn(async move {
            while let Some(Request::Client { data, reply }) = requests.recv().await {
                let _ = reply.send(service.handle(data, &mut receiver, &mut engine, 0));
            }
        });
        Self {
            address,
            state,
            server,
            owner,
        }
    }
    async fn connect(&self) -> Client {
        let (mut client, _) = connect_async(format!("ws://{}", self.address))
            .await
            .unwrap();
        let Message::Text(info) = next(&mut client).await else {
            panic!("missing backend info")
        };
        let info: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(info["backend"], "rust");
        for i in 1..=slimevr_server::steamvr::ROLES.len() {
            let Message::Text(config) = next(&mut client).await else {
                panic!("missing legacy config")
            };
            let config: serde_json::Value = serde_json::from_str(&config).unwrap();
            assert_eq!(config["tracker_id"], format!("SlimeVR Tracker {i}"));
        }
        client
    }
    fn publish(&self, at: u64) {
        let mut live = (**self.state.borrow()).clone();
        live.at = at;
        self.state.send_replace(Arc::new(live));
    }
}

async fn next(client: &mut Client) -> Message {
    tokio::time::timeout(Duration::from_secs(3), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
async fn heartbeat(client: &mut Client) {
    client
        .send(Message::Binary(
            protocol::rpc_frame(rpc::RpcMessage::HeartbeatRequest, 17, |f| {
                rpc::HeartbeatRequest::create(f, &Default::default()).as_union_value()
            })
            .into(),
        ))
        .await
        .unwrap();
    let Message::Binary(bytes) = next(client).await else {
        panic!("missing heartbeat")
    };
    let header = fb::root::<sx::MessageBundle>(&bytes)
        .unwrap()
        .rpc_msgs()
        .unwrap()
        .get(0);
    assert_eq!(header.message_type(), rpc::RpcMessage::HeartbeatResponse);
    assert_eq!(header.tx_id().unwrap().id(), 17);
}

fn feed(start: bool) -> Message {
    let mut f = fb::FlatBufferBuilder::new();
    let config = df::DataFeedConfig::create(
        &mut f,
        &df::DataFeedConfigArgs {
            minimum_time_since_last: 50,
            bone_mask: true,
            ..Default::default()
        },
    );
    let (kind, payload) = if start {
        let configs = f.create_vector(&[config]);
        (
            df::DataFeedMessage::StartDataFeed,
            df::StartDataFeed::create(
                &mut f,
                &df::StartDataFeedArgs {
                    data_feeds: Some(configs),
                },
            )
            .as_union_value(),
        )
    } else {
        (
            df::DataFeedMessage::PollDataFeed,
            df::PollDataFeed::create(
                &mut f,
                &df::PollDataFeedArgs {
                    config: Some(config),
                },
            )
            .as_union_value(),
        )
    };
    let header = df::DataFeedMessageHeader::create(
        &mut f,
        &df::DataFeedMessageHeaderArgs {
            message_type: kind,
            message: Some(payload),
        },
    );
    let headers = f.create_vector(&[header]);
    let bundle = sx::MessageBundle::create(
        &mut f,
        &sx::MessageBundleArgs {
            data_feed_msgs: Some(headers),
            ..Default::default()
        },
    );
    f.finish(bundle, None);
    Message::Binary(f.finished_data().to_vec().into())
}

async fn expect_feed(client: &mut Client) {
    let Message::Binary(bytes) = next(client).await else {
        panic!("missing feed")
    };
    let bundle = fb::root::<sx::MessageBundle>(&bytes).unwrap();
    assert_eq!(
        bundle.data_feed_msgs().unwrap().get(0).message_type(),
        df::DataFeedMessage::DataFeedUpdate
    );
}

#[tokio::test]
async fn websocket_rpc_poll_and_scheduled_feeds_keep_connection_alive() {
    let server = Running::start().await;
    let mut client = server.connect().await;
    heartbeat(&mut client).await;
    client.send(feed(false)).await.unwrap();
    expect_feed(&mut client).await;
    client.send(feed(true)).await.unwrap();
    heartbeat(&mut client).await; // Ordered barrier: the feed subscription is installed.
    server.publish(10);
    expect_feed(&mut client).await;
    server.publish(20);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), client.next())
            .await
            .is_err()
    );
    server.publish(70);
    expect_feed(&mut client).await;
    heartbeat(&mut client).await;
    client.close(None).await.unwrap();
}

fn topic(subscribe: bool) -> Message {
    let mut f = fb::FlatBufferBuilder::new();
    let organization = f.create_string("slimevr-test");
    let app_name = f.create_string("overlay");
    let topic = f.create_string("mode");
    let id = ps::TopicId::create(
        &mut f,
        &ps::TopicIdArgs {
            organization: Some(organization),
            app_name: Some(app_name),
            topic: Some(topic),
        },
    )
    .as_union_value();
    let (kind, payload) = if subscribe {
        (
            ps::PubSubUnion::SubscriptionRequest,
            ps::SubscriptionRequest::create(
                &mut f,
                &ps::SubscriptionRequestArgs {
                    topic_type: ps::Topic::TopicId,
                    topic: Some(id),
                },
            )
            .as_union_value(),
        )
    } else {
        (
            ps::PubSubUnion::Message,
            ps::Message::create(
                &mut f,
                &ps::MessageArgs {
                    topic_type: ps::Topic::TopicId,
                    topic: Some(id),
                    ..Default::default()
                },
            )
            .as_union_value(),
        )
    };
    let header = ps::PubSubHeader::create(
        &mut f,
        &ps::PubSubHeaderArgs {
            u_type: kind,
            u: Some(payload),
        },
    );
    let headers = f.create_vector(&[header]);
    let bundle = sx::MessageBundle::create(
        &mut f,
        &sx::MessageBundleArgs {
            pub_sub_msgs: Some(headers),
            ..Default::default()
        },
    );
    f.finish(bundle, None);
    Message::Binary(f.finished_data().to_vec().into())
}

#[tokio::test]
async fn topic_subscription_is_per_connection_and_does_not_echo_to_sender() {
    let server = Running::start().await;
    let mut sender = server.connect().await;
    let mut subscriber = server.connect().await;
    let mut unrelated = server.connect().await;
    for client in [&mut sender, &mut subscriber] {
        client.send(topic(true)).await.unwrap();
        let Message::Binary(bytes) = next(client).await else {
            panic!("missing topic mapping")
        };
        let bundle = fb::root::<sx::MessageBundle>(&bytes).unwrap();
        assert_eq!(
            bundle.pub_sub_msgs().unwrap().get(0).u_type(),
            ps::PubSubUnion::TopicMapping
        );
    }
    sender.send(topic(false)).await.unwrap();
    let Message::Binary(bytes) = next(&mut subscriber).await else {
        panic!("missing topic message")
    };
    let bundle = fb::root::<sx::MessageBundle>(&bytes).unwrap();
    assert_eq!(
        bundle.pub_sub_msgs().unwrap().get(0).u_type(),
        ps::PubSubUnion::Message
    );
    for client in [&mut sender, &mut unrelated] {
        assert!(
            tokio::time::timeout(Duration::from_millis(100), client.next())
                .await
                .is_err()
        );
        heartbeat(client).await;
    }
}
