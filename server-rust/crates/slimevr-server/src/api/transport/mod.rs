//! Bounded WebSocket connections. Per-client subscriptions stay in Session.
mod session;
use super::{pubsub, LiveState, Request, Wire};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use session::Session;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc, watch, Semaphore},
};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
    WebSocketStream,
};

type ConnectionError = Box<dyn std::error::Error + Send + Sync>;
type Socket = WebSocketStream<TcpStream>;

pub async fn serve(
    bind: SocketAddr,
    commands: mpsc::Sender<Request>,
    state: watch::Receiver<Arc<LiveState>>,
    events: broadcast::Sender<Wire>,
    hub: pubsub::Hub,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), std::io::Error> {
    let listener = TcpListener::bind(bind).await?;
    let address = listener.local_addr()?;
    let permits = Arc::new(Semaphore::new(16));
    let broker = hub.broker.clone();
    let task = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let Ok((socket, peer)) = accepted else { break };
                    let Ok(permit) = permits.clone().try_acquire_owned() else { continue };
                    let commands = commands.clone();
                    let state = state.clone();
                    let publisher = events.clone();
                    let events = events.subscribe();
                    let broker = broker.clone();
                    let client_id = hub.client_id();
                    clients.spawn(async move {
                        let _permit = permit;
                        if let Err(error) = connection(socket, commands, state, events, publisher, broker, client_id).await {
                            crate::logging::diagnostic(crate::log_level::LogLevel::Warn,
                                &json!({"type":"api_connection_error", "client_id":client_id, "peer":peer, "error":error.to_string()}));
                        }
                    });
                }
                _ = clients.join_next(), if !clients.is_empty() => {}
            }
        }
    });
    Ok((address, task))
}

async fn connection(
    socket: TcpStream,
    commands: mpsc::Sender<Request>,
    mut state: watch::Receiver<Arc<LiveState>>,
    mut events: broadcast::Receiver<Wire>,
    publisher: broadcast::Sender<Wire>,
    broker: Arc<std::sync::Mutex<pubsub::Broker>>,
    client_id: u64,
) -> Result<(), ConnectionError> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(8 * 1024 * 1024))
        .max_frame_size(Some(8 * 1024 * 1024));
    let mut socket = tokio::time::timeout(
        Duration::from_secs(5),
        accept_async_with_config(socket, Some(config)),
    )
    .await??;
    crate::logging::diagnostic(
        crate::log_level::LogLevel::Info,
        &json!({"type":"api_connected", "client_id":client_id}),
    );
    let initial = state.borrow().clone();
    let mut session = Session::new(commands, publisher, broker, client_id, &initial);
    socket.send(backend_info(&initial)).await?;
    for (i, (_, role, _)) in crate::steamvr::ROLES.iter().enumerate() {
        socket
            .send(Message::Text(
                json!({
                    "type":"config", "tracker_id":format!("SlimeVR Tracker {}", i + 1),
                    "location":role, "tracker_type":role
                })
                .to_string()
                .into(),
            ))
            .await?;
    }
    loop {
        tokio::select! {
            input = socket.next() => {
                let Some(input) = input else { break };
                if !session.receive(&mut socket, input?, &state).await? { break }
            }
            changed = state.changed() => {
                if changed.is_err() { break }
                let live = state.borrow().clone();
                session.publish(&mut socket, &live).await?;
            }
            event = events.recv() => {
                match event {
                    Ok(message) => session.forward(&mut socket, message).await?,
                    Err(broadcast::error::RecvError::Lagged(_)) => return Err("client cannot keep up with server events".into()),
                    Err(_) => break,
                }
            }
        }
    }
    crate::logging::diagnostic(
        crate::log_level::LogLevel::Info,
        &json!({"type":"api_disconnected", "client_id":client_id}),
    );
    Ok(())
}

fn backend_info(state: &LiveState) -> Message {
    let mut capabilities = vec![
        "datafeed",
        "assignment",
        "reset",
        "pause",
        "settings",
        "skeleton",
        "autobone",
        "height",
        "pose_input",
        "legacy_websocket",
        "bvh",
        "derived_velocity",
        "extended_calibration",
        "magnetometer_control",
        "tap_setup",
        "autobone_recordings",
        "hid",
        "compatible_settings",
        "serial",
        "firmware",
        "provisioning",
        "external_trackers",
        "steamvr_feeder",
        "osc",
        "oscquery",
        "vmc",
        "vrchat_config",
        "overlay",
        "hotkeys",
    ];
    if state.steam_vr.available {
        capabilities.extend(["steamvr", "steamvr_management"]);
    }
    Message::Text(
        json!({"type":"backend_info","backend":"rust","capabilities":capabilities,
        "persistent":state.persistent,"steamvr":state.steam_vr,
        "steamvr_driver":state.driver_status,"driver_error":state.driver_status.registration_error,
        "driver_notice":state.driver_status.registration_notice})
        .to_string()
        .into(),
    )
}
