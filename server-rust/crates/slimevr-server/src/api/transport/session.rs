//! Connection-local feed, topic and serial subscriptions; never owns the pose engine.
use super::{backend_info, ConnectionError, Socket};
use crate::api::{protocol, pubsub, LiveState, Request, Wire};
use futures_util::SinkExt;
use solarxr_protocol::{self as sx, flatbuffers as fb, rpc};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Session {
    commands: mpsc::Sender<Request>,
    publisher: broadcast::Sender<Wire>,
    broker: Arc<Mutex<pubsub::Broker>>,
    client_id: u64,
    subscriptions: BTreeSet<u16>,
    feeds: Vec<protocol::Feed>,
    use_serial: bool,
    use_provisioning: bool,
    steam_status: crate::steamvr::Status,
    driver_status: crate::steamvr::manager::DriverStatus,
}

impl Session {
    pub(super) fn new(
        commands: mpsc::Sender<Request>,
        publisher: broadcast::Sender<Wire>,
        broker: Arc<Mutex<pubsub::Broker>>,
        client_id: u64,
        initial: &LiveState,
    ) -> Self {
        Self {
            commands,
            publisher,
            broker,
            client_id,
            subscriptions: BTreeSet::new(),
            feeds: Vec::new(),
            use_serial: false,
            use_provisioning: false,
            steam_status: initial.steam_vr.clone(),
            driver_status: initial.driver_status.clone(),
        }
    }

    pub(super) async fn receive(
        &mut self,
        socket: &mut Socket,
        input: Message,
        state: &watch::Receiver<Arc<LiveState>>,
    ) -> Result<bool, ConnectionError> {
        match input {
            Message::Close(_) => return Ok(false),
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await?,
            Message::Binary(ref bytes) => {
                if self.binary(socket, bytes, state).await? {
                    self.request(socket, input).await?;
                }
            }
            Message::Text(_) => self.request(socket, input).await?,
            _ => {}
        }
        Ok(true)
    }

    async fn request(&self, socket: &mut Socket, data: Message) -> Result<(), ConnectionError> {
        let (reply, response) = oneshot::channel();
        self.commands.send(Request::Client { data, reply }).await?;
        for message in tokio::time::timeout(Duration::from_secs(5), response).await?? {
            socket.send(message.message()).await?;
        }
        Ok(())
    }

    async fn binary(
        &mut self,
        socket: &mut Socket,
        bytes: &[u8],
        state: &watch::Receiver<Arc<LiveState>>,
    ) -> Result<bool, ConnectionError> {
        let bundle = fb::root::<sx::MessageBundle>(bytes).map_err(|_| "invalid SolarXR frame")?;
        if let Some(headers) = bundle.rpc_msgs() {
            for h in headers {
                match h.message_type() {
                    rpc::RpcMessage::OpenSerialRequest => self.use_serial = true,
                    rpc::RpcMessage::CloseSerialRequest => self.use_serial = false,
                    rpc::RpcMessage::StartWifiProvisioningRequest => self.use_provisioning = true,
                    rpc::RpcMessage::StopWifiProvisioningRequest => self.use_provisioning = false,
                    _ => {}
                }
            }
        }
        if let Some(headers) = bundle.data_feed_msgs() {
            if headers.len() > 8 {
                return Err("too many data feeds".into());
            }
            for h in headers {
                match h.message_type() {
                    sx::data_feed::DataFeedMessage::StartDataFeed => {
                        let configs = h
                            .message_as_start_data_feed()
                            .and_then(|s| s.data_feeds())
                            .ok_or("missing data feeds")?;
                        if configs.len() > 8 {
                            return Err("too many data feeds".into());
                        }
                        self.feeds = configs
                            .iter()
                            .enumerate()
                            .map(|(i, c)| protocol::Feed::from_config(i as u8, c))
                            .collect();
                    }
                    sx::data_feed::DataFeedMessage::PollDataFeed => {
                        let c = h
                            .message_as_poll_data_feed()
                            .and_then(|p| p.config())
                            .ok_or("missing feed config")?;
                        let live = state.borrow().clone();
                        socket
                            .send(feed_frame(&protocol::Feed::from_config(0, c), &live))
                            .await?;
                    }
                    _ => return Err("unsupported data feed message".into()),
                }
            }
        }
        if let Some(headers) = bundle.pub_sub_msgs() {
            let dispatched = {
                self.broker
                    .lock()
                    .map_err(|_| "topic broker unavailable")?
                    .dispatch(headers, &mut self.subscriptions)?
            };
            for bytes in dispatched.replies {
                socket.send(Message::Binary(bytes.into())).await?;
            }
            for (handle, bytes) in dispatched.messages {
                let _ = self.publisher.send(Wire::PubSub {
                    origin: self.client_id,
                    handle,
                    bytes,
                });
            }
        }
        Ok(bundle.rpc_msgs().is_some())
    }

    pub(super) async fn publish(
        &mut self,
        socket: &mut Socket,
        live: &LiveState,
    ) -> Result<(), ConnectionError> {
        if self.steam_status != live.steam_vr || self.driver_status != live.driver_status {
            self.steam_status = live.steam_vr.clone();
            self.driver_status = live.driver_status.clone();
            tokio::time::timeout(Duration::from_secs(2), socket.send(backend_info(live))).await??;
        }
        for feed in &mut self.feeds {
            if live.at >= feed.next_at {
                feed.next_at = live.at.saturating_add(feed.minimum_ms);
                tokio::time::timeout(Duration::from_secs(2), socket.send(feed_frame(feed, live)))
                    .await??;
            }
        }
        Ok(())
    }

    pub(super) async fn forward(
        &self,
        socket: &mut Socket,
        message: Wire,
    ) -> Result<(), ConnectionError> {
        if let Wire::PubSub { origin, handle, .. } = &message {
            if *origin == self.client_id || !self.subscriptions.contains(handle) {
                return Ok(());
            }
        }
        if (matches!(message, Wire::Serial(_)) && !self.use_serial)
            || (matches!(message, Wire::Provisioning(_)) && !self.use_provisioning)
        {
            return Ok(());
        }
        tokio::time::timeout(Duration::from_secs(2), socket.send(message.message())).await??;
        Ok(())
    }
}

fn feed_frame(feed: &protocol::Feed, live: &LiveState) -> Message {
    Message::Binary(
        protocol::data_frame(
            feed,
            &live.devices,
            &live.config.device_ids,
            &live.pose,
            &live.config.pose,
            &live.config.tracker_names,
            &live.external,
        )
        .into(),
    )
}
