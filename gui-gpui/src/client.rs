//! One background thread, bounded commands, and one replaceable UI snapshot.
use crate::{
    log_level::LogLevel,
    protocol::{self, Command, Feed, ResetProgress, Settings, Update, Vrchat},
};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::{Message, protocol::WebSocketConfig};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Connection {
    #[default]
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Clone, Debug, Serialize)]
pub struct Pending {
    pub tx: u32,
    #[serde(skip)]
    pub command: Command,
    #[serde(skip)]
    pub started: Instant,
}

#[derive(Clone, Debug, Serialize)]
pub struct RpcRecord {
    pub sequence: u64,
    pub tx: u32,
    pub value: Arc<serde_json::Value>,
}
#[derive(Clone, Debug)]
pub struct Event {
    pub session: u64,
    pub sequence: u64,
    pub name: String,
    pub tx: u32,
    pub value: Arc<serde_json::Value>,
    pub bytes: Option<Arc<[u8]>>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Snapshot {
    pub revision: u64,
    pub session: u64,
    pub connection: Connection,
    pub feed: Option<Feed>,
    pub paused: Option<bool>,
    pub settings: Option<Settings>,
    pub vrchat: Option<Vrchat>,
    pub reset: Option<ResetProgress>,
    pub pending: Option<Pending>,
    pub last_error: Option<String>,
    pub diagnostics: VecDeque<String>,
    pub rpc: BTreeMap<String, RpcRecord>,
    pub serial_log: String,
    pub serial_device: Option<serde_json::Value>,
    pub serial_closed_locally: bool,
    pub event_sequence: u64,
}

type ResetListener = Arc<dyn Fn(u64, &ResetProgress) + Send + Sync>;

struct Shared {
    state: Mutex<Snapshot>,
    level: LogLevel,
    events: Mutex<VecDeque<Event>>,
    telemetry_ms: AtomicU16,
    bone_ms: AtomicU16,
    bones_enabled: AtomicBool,
    events_overflow: AtomicBool,
    reset_listener: Mutex<Option<ResetListener>>,
}
impl Shared {
    fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut s);
        s.revision = s.revision.wrapping_add(1);
    }
    fn log(&self, level: LogLevel, message: String) {
        if !self.level.allows(level) {
            return;
        }
        eprintln!("[{}] [solarxr] {message}", level.as_str());
        crate::logging::write(level, "solarxr", &message);
        self.update(|s| {
            if s.diagnostics.len() == 64 {
                s.diagnostics.pop_front();
            }
            s.diagnostics
                .push_back(format!("[{}] {message}", level.as_str()));
        });
    }
    fn apply(&self, updates: Vec<Update>) {
        let mut resets = Vec::new();
        self.update(|s| {
            for update in updates {
                match update {
                    Update::Bones(bones)=>{if let Some(feed)=&mut s.feed{feed.bones=bones;}else{s.feed=Some(Feed{bones,..Default::default()});}}
                    Update::Feed(mut feed) => {
                        if feed.bones.is_empty(){feed.bones=s.feed.as_ref().map(|f|f.bones.clone()).unwrap_or_default();}
                        if s.pending.as_ref().is_some_and(|p| match p.command {
                            Command::Assign { key, body } => feed.trackers.iter().any(|t| t.key == key && t.body == body),
                            _ => false,
                        }) { s.pending = None; }
                        s.feed = Some(feed);
                    }
                    Update::Reset(reset) => {
                        let matches = s.pending.as_ref().is_some_and(|p| matches!(p.command, Command::Reset(kind) if p.tx == reset.tx && kind.wire().0 == reset.kind));
                        if matches || s.pending.is_none() {
                            if matches && reset.done { s.pending = None; }
                            resets.push((s.session, reset.clone()));
                            s.reset = Some(reset);
                        }
                    }
                    Update::Paused(paused) => {
                        if s.pending.as_ref().is_some_and(|p| matches!(p.command, Command::Pause(want) if want == paused)) { s.pending = None; }
                        s.paused = Some(paused);
                    }
                    Update::Settings(settings) => s.settings = Some(settings),
                    Update::Vrchat(vrchat) => {
                        if s.pending.as_ref().is_some_and(|p| match &p.command {
                            Command::MuteVrchat(key) => s.vrchat.as_ref().and_then(|v| v.rows.iter().find(|r| r.key == *key)).zip(vrchat.rows.iter().find(|r| r.key == *key)).is_some_and(|(old, new)| old.muted != new.muted),
                            _ => false,
                        }) { s.pending = None; }
                        s.vrchat = Some(vrchat);
                    }
                    Update::Heartbeat => (),
                }
            }
        });
        // Deliver audio feedback on the receive thread, outside the snapshot lock.
        // A slow/minimized GUI must not delay or coalesce countdown cues.
        let listener = self
            .reset_listener
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(listener) = listener {
            for (session, reset) in resets {
                listener(session, &reset);
            }
        }
    }
    fn rpc(&self, messages: Vec<(String, u32, serde_json::Value)>) {
        self.rpc_files(messages, Vec::new());
    }
    fn rpc_files(&self, messages: Vec<(String, u32, serde_json::Value)>, files: Vec<Arc<[u8]>>) {
        let mut files = files.into_iter();
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        self.update(|s| {
            for (name, tx, value) in messages {
                if name == "HeartbeatResponse" {
                    continue;
                }
                if name == "SerialUpdateResponse" && !s.serial_closed_locally {
                    if value["closed"] == true {
                        s.serial_device = None;
                    } else if value["device"].is_object() {
                        s.serial_device = Some(value["device"].clone());
                    }
                    if let Some(log) = value["log"].as_str() {
                        s.serial_log.push_str(log);
                    }
                    if s.serial_log.len() > 65536 {
                        let mut cut = s.serial_log.len() - 65536;
                        while !s.serial_log.is_char_boundary(cut) {
                            cut += 1;
                        }
                        s.serial_log.drain(..cut);
                    }
                }
                s.event_sequence = s.event_sequence.wrapping_add(1);
                let sequence = s.event_sequence;
                let value = Arc::new(value);
                s.rpc.insert(
                    name.clone(),
                    RpcRecord {
                        sequence,
                        tx,
                        value: value.clone(),
                    },
                );
                if !matches!(
                    name.as_str(),
                    "ResetResponse"
                        | "TrackingPauseStateResponse"
                        | "UnknownDeviceHandshakeNotification"
                        | "TapDetectionSetupNotification"
                        | "AutoBoneEpochResponse"
                        | "AutoBoneProcessStatusResponse"
                        | "UserHeightRecordingStatusResponse"
                        | "FirmwareUpdateStatusResponse"
                        | "WifiProvisioningStatusResponse"
                        | "SaveFileNotification"
                        | "BackendNotice"
                        | "PubSub"
                        | "NewSerialDeviceResponse"
                        | "SkeletonConfigResponse"
                        | "MagToggleResponse"
                ) {
                    continue;
                }
                // Backpressure for progress is explicit. Completion, alerts and files cannot
                // silently disappear behind an incoming pose frame.
                if events.len() >= 1024 {
                    if let Some(index) = events.iter().position(|e| {
                        matches!(
                            e.name.as_str(),
                            "AutoBoneEpochResponse" | "SerialUpdateResponse"
                        )
                    }) {
                        events.remove(index);
                    } else {
                        s.last_error =
                            Some("UI event queue overflow; reconnect to restore state".into());
                        self.events_overflow.store(true, Ordering::Relaxed);
                        break;
                    }
                }
                let bytes = if name == "SaveFileNotification" {
                    files.next()
                } else {
                    None
                };
                events.push_back(Event {
                    session: s.session,
                    sequence,
                    name,
                    tx,
                    value,
                    bytes,
                });
            }
        });
    }
    fn disconnect(&self, error: String) {
        self.log(LogLevel::Warn, error.clone());
        self.update(|s| {
            s.connection = Connection::Disconnected;
            s.feed = None;
            s.paused = None;
            s.settings = None;
            s.vrchat = None;
            s.reset = None;
            s.rpc.clear();
            s.serial_device = None;
            s.last_error = Some(if s.pending.take().is_some() {
                format!("{error}; operation interrupted, not replayed")
            } else {
                error
            });
        });
    }
}

struct Queued {
    session: u64,
    tx: u32,
    command: Command,
}

pub struct Client {
    shared: Arc<Shared>,
    sender: mpsc::Sender<Queued>,
    stop: watch::Sender<bool>,
    next_tx: AtomicU32,
    thread: Option<thread::JoinHandle<()>>,
}
impl Client {
    pub fn connect(url: String, level: LogLevel) -> Result<Self, String> {
        if !url.starts_with("ws://") {
            return Err("The native frontend supports ws:// SolarXR endpoints only".into());
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(Snapshot::default()),
            level,
            events: Mutex::new(VecDeque::new()),
            telemetry_ms: AtomicU16::new(100),
            bone_ms: AtomicU16::new(25),
            bones_enabled: AtomicBool::new(true),
            events_overflow: AtomicBool::new(false),
            reset_listener: Mutex::new(None),
        });
        let (sender, receiver) = mpsc::channel(16);
        let (stop, stopping) = watch::channel(false);
        let worker = shared.clone();
        let thread = thread::Builder::new()
            .name("slimevr-solarxr".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt.block_on(run(url, worker, receiver, stopping)),
                    Err(e) => worker.disconnect(e.to_string()),
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            sender,
            stop,
            next_tx: AtomicU32::new(100),
            thread: Some(thread),
        })
    }
    /// Register a nonblocking observer for accepted reset packets. It runs on
    /// the receive thread; dispatch playback without doing audio I/O here.
    pub fn on_reset(&self, listener: impl Fn(u64, &ResetProgress) + Send + Sync + 'static) {
        *self
            .shared
            .reset_listener
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(listener));
    }
    pub fn snapshot(&self) -> Snapshot {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    /// Avoid cloning a full UI snapshot when no packets have changed it.
    pub fn snapshot_after(&self, revision: u64) -> Option<Snapshot> {
        let state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        (state.revision != revision).then(|| state.clone())
    }
    pub fn drain_events(&self) -> Vec<Event> {
        self.shared
            .events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain(..)
            .collect()
    }
    pub fn rpc(&self, name: &str, value: serde_json::Value) -> Result<u32, String> {
        crate::rpc_generated::encode_rpc(name, &value, 0)?;
        self.send(Command::Rpc {
            name: name.into(),
            value,
        })
    }
    pub fn configure_feed(&self, telemetry: u16, bones: u16, enabled: bool) -> Result<(), String> {
        self.shared
            .telemetry_ms
            .store(telemetry.clamp(1, 5000), Ordering::Relaxed);
        self.shared
            .bone_ms
            .store(bones.clamp(1, 5000), Ordering::Relaxed);
        self.shared.bones_enabled.store(enabled, Ordering::Relaxed);
        if self.snapshot().connection == Connection::Connected {
            self.send(Command::FeedConfig(telemetry, bones, enabled))?;
        }
        Ok(())
    }
    pub fn batch(&self, requests: Vec<(String, serde_json::Value)>) -> Result<u32, String> {
        crate::rpc_generated::encode_rpc_batch(&requests, 0)?;
        self.send(Command::Batch(requests))
    }
    pub fn send(&self, command: Command) -> Result<u32, String> {
        let mut s = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.connection != Connection::Connected {
            return Err("Backend is disconnected".into());
        }
        let read = matches!(
            command,
            Command::ReadSettings
                | Command::ReadVrchat
                | Command::Rpc { .. }
                | Command::PubSub(_)
                | Command::Batch(_)
                | Command::FeedConfig(..)
        );
        match &command {
            Command::Batch(requests) => {
                crate::rpc_generated::encode_rpc_batch(requests, 0)?;
            }
            Command::Rpc { name, value } => {
                crate::rpc_generated::encode_rpc(name, value, 0)?;
            }
            Command::PubSub(value) => {
                crate::rpc_generated::encode_pubsub(value)?;
            }
            _ => (),
        }
        if !read && s.pending.is_some() {
            return Err("Wait for the current operation to complete".into());
        }
        match &command {
            Command::Rpc { .. }
            | Command::PubSub(_)
            | Command::Batch(_)
            | Command::FeedConfig(..) => (),
            Command::Reset(kind) => {
                let feed = s.feed.as_ref().ok_or("Waiting for device data")?;
                if *kind == protocol::ResetKind::Yaw && !feed.can_yaw {
                    return Err("Perform a full reset first".into());
                }
                if *kind == protocol::ResetKind::Mounting && !feed.can_mount {
                    return Err("Perform a full reset first".into());
                }
            }
            Command::Pause(_) if s.paused.is_none() => return Err("Waiting for pause state".into()),
            Command::Assign { key, body } => {
                if key.device == 0
                    || !s
                        .feed
                        .as_ref()
                        .is_some_and(|f| f.trackers.iter().any(|t| t.key == *key))
                {
                    return Err("Tracker is unavailable".into());
                }
                if solarxr_protocol::datatypes::BodyPart(*body)
                    .variant_name()
                    .is_none()
                {
                    return Err("Unknown body part".into());
                }
            }
            Command::MuteVrchat(key)
                if !s
                    .vrchat
                    .as_ref()
                    .is_some_and(|v| v.supported && v.rows.iter().any(|r| r.key == *key)) =>
            {
                return Err("VRChat setting is unavailable".into());
            }
            _ => (),
        }
        let tx = self.next_tx.fetch_add(1, Ordering::Relaxed);
        self.sender
            .try_send(Queued {
                session: s.session,
                tx,
                command: command.clone(),
            })
            .map_err(|e| e.to_string())?;
        // The current backend unsubscribes before sending the serial close
        // notification. Retire the local port and ignore late stream frames.
        if let Command::Rpc { name, .. } = &command {
            match name.as_str() {
                "CloseSerialRequest" => {
                    s.serial_device = None;
                    s.serial_closed_locally = true;
                }
                "OpenSerialRequest" => s.serial_closed_locally = false,
                _ => (),
            }
        }
        if !read {
            s.pending = Some(Pending {
                tx,
                command,
                started: Instant::now(),
            });
            s.reset = None;
        }
        s.last_error = None;
        s.revision = s.revision.wrapping_add(1);
        Ok(tx)
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn run(
    url: String,
    shared: Arc<Shared>,
    mut commands: mpsc::Receiver<Queued>,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        if *stop.borrow() {
            return;
        }
        shared.update(|s| s.connection = Connection::Connecting);
        let config = WebSocketConfig::default()
            .max_message_size(Some(64 * 1024 * 1024))
            .max_frame_size(Some(64 * 1024 * 1024));
        let connection = tokio::select! {
            _ = stop.changed() => return,
            connection = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async_with_config(&url, Some(config), false)) => connection,
        };
        let error = match connection {
            Ok(Ok((mut socket, _))) => {
                shared.events_overflow.store(false, Ordering::Relaxed);
                // Startup only restores reads and subscriptions. Mutations never live across sessions.
                let mut startup = vec![
                    protocol::subscribe_config(
                        shared.telemetry_ms.load(Ordering::Relaxed),
                        shared.bone_ms.load(Ordering::Relaxed),
                        shared.bones_enabled.load(Ordering::Relaxed),
                    ),
                    protocol::pause_request(1),
                    protocol::encode(&Command::ReadSettings, 2),
                    protocol::encode(&Command::ReadVrchat, 3),
                ];
                for (index, name) in [
                    "SkeletonConfigRequest",
                    "SerialDevicesRequest",
                    "RecordBVHStatusRequest",
                    "TrackingChecklistRequest",
                    "KeybindRequest",
                    "OverlayDisplayModeRequest",
                    "ServerInfosRequest",
                    "InstalledInfoRequest",
                    "MagToggleRequest",
                ]
                .into_iter()
                .enumerate()
                {
                    if let Ok(frame) = crate::rpc_generated::encode_rpc(
                        name,
                        &serde_json::json!({}),
                        10 + index as u32,
                    ) {
                        startup.push(frame);
                    }
                }
                startup.push(crate::rpc_generated::encode_pubsub(&serde_json::json!({"u":{"type":"SubscriptionRequest","value":{"topic":{"type":"TopicId","value":{"organization":"slimevr.dev","app_name":"overlay","topic":"display_settings"}}}}})).expect("Static overlay subscription is valid"));
                startup.push(
                    crate::rpc_generated::encode_pubsub(&crate::overlay::message(None))
                        .expect("Static overlay query is valid"),
                );
                let mut failed = None;
                for frame in startup {
                    let result = tokio::select! {
                        _ = stop.changed() => return,
                        sent = tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Binary(frame.into()))) => sent,
                    };
                    if !matches!(result, Ok(Ok(()))) {
                        failed = Some("Failed to initialize SolarXR subscription".to_owned());
                        break;
                    }
                }
                if let Some(error) = failed {
                    error
                } else {
                    let mut session = 0;
                    shared.update(|s| {
                        s.session += 1;
                        session = s.session;
                        s.connection = Connection::Connected;
                        s.last_error = None;
                    });
                    shared.log(LogLevel::Info, format!("Connected to {url}"));
                    let mut last_received = Instant::now();
                    let mut heartbeat = tokio::time::interval(Duration::from_secs(3));
                    loop {
                        tokio::select! {
                            _ = stop.changed() => {
                                let _ = tokio::time::timeout(Duration::from_millis(300), socket.close(None)).await;
                                return;
                            },
                            request = commands.recv() => {
                                let Some(request) = request else { return; };
                                if request.session != session { continue; }
                                shared.log(LogLevel::Debug, format!("tx={} {}", request.tx, match &request.command { Command::Rpc { name, .. } => name.as_str(), Command::PubSub(_) => "PubSub", Command::Batch(_) => "batch", Command::FeedConfig(..)=>"feed", _ => "tracker command" }));
                                let frame = protocol::encode(&request.command, request.tx);
                                match tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Binary(frame.into()))).await {
                                    Ok(Ok(())) => (),
                                    _ => break "SolarXR send timed out or failed".into(),
                                }
                            }
                            message = socket.next() => match message {
                                Some(Ok(Message::Binary(bytes))) => {
                                    let bundle=match solarxr_protocol::flatbuffers::root::<solarxr_protocol::MessageBundle>(&bytes){Ok(b)=>b,Err(e)=>break format!("Invalid SolarXR frame: {e}")};
                                    match crate::rpc_generated::decode_rpc_verified(bundle) { Ok(updates) => shared.rpc_files(updates,protocol::files_verified(bundle).unwrap_or_default()), Err(error) => break error }
                                    match crate::rpc_generated::decode_pubsub_verified(bundle) {
                                        Ok(updates) => shared.rpc(updates.into_iter().map(|v| ("PubSub".into(), 0, v)).collect()),
                                        Err(error) => break error
                                    }
                                    match protocol::decode_verified(bundle) { Ok(updates) => shared.apply(updates), Err(error) => break error }
                                    if shared.events_overflow.load(Ordering::Relaxed){break "UI event queue overflow; operation interrupted, reconnecting".into();}
                                    last_received = Instant::now();
                                }
                                Some(Ok(Message::Text(text))) => {
                                    if shared.events_overflow.load(Ordering::Relaxed){break "UI event queue overflow; operation interrupted, reconnecting".into();}
                                    last_received = Instant::now();
                                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                                        {
                                        if value["type"] != "backend_error" {
                                            shared.rpc(vec![("BackendNotice".into(), 0, value)]);
                                            continue;
                                        }
                                            let error = value["message"].as_str().unwrap_or("Backend request failed").to_string();
                                            shared.log(LogLevel::Warn, error.clone());
                                            shared.update(|s| { s.last_error = Some(error); s.pending = None; });
                                    }
                                }
                                Some(Ok(Message::Ping(_) | Message::Pong(_))) => last_received = Instant::now(),
                                Some(Ok(Message::Close(_))) | None => break "SolarXR connection closed".into(),
                                Some(Err(e)) => break e.to_string(),
                                _ => (),
                            },
                            _ = heartbeat.tick() => {
                                if last_received.elapsed() > Duration::from_secs(15) { break "No SolarXR response for 15 seconds".into(); }
                                let mut timed_out = false;
                                shared.update(|s| {
                                    if s.pending.as_ref().is_some_and(|p| p.started.elapsed() > Duration::from_secs(if matches!(p.command, Command::Reset(_)) { 75 } else { 5 })) {
                                        s.pending = None; s.last_error = Some("Operation was not confirmed by the backend".into()); timed_out = true;
                                    }
                                });
                                if timed_out { shared.log(LogLevel::Warn, "Operation confirmation timed out".into()); }
                                match tokio::time::timeout(Duration::from_secs(2), socket.send(Message::Binary(protocol::heartbeat(4).into()))).await {
                                    Ok(Ok(())) => (),
                                    _ => break "SolarXR heartbeat failed".into(),
                                }
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => e.to_string(),
            Err(_) => "SolarXR connection timed out".into(),
        };
        shared.disconnect(error);
        while commands.try_recv().is_ok() {}
        tokio::select! { _ = stop.changed() => return, _ = tokio::time::sleep(Duration::from_secs(1)) => () }
    }
}

#[cfg(test)]
mod serial_state_tests {
    use super::*;
    #[test]
    fn wifi_status_reaches_ui_events_with_session_and_transaction() {
        let shared = Shared {
            state: Mutex::new(Snapshot {
                session: 4,
                ..Default::default()
            }),
            level: LogLevel::Error,
            events: Mutex::new(VecDeque::new()),
            telemetry_ms: AtomicU16::new(100),
            bone_ms: AtomicU16::new(25),
            bones_enabled: AtomicBool::new(false),
            events_overflow: AtomicBool::new(false),
            reset_listener: Mutex::new(None),
        };
        shared.rpc(vec![
            (
                "WifiProvisioningStatusResponse".into(),
                42,
                serde_json::json!({"status": 1}),
            ),
            (
                "WifiProvisioningStatusResponse".into(),
                0,
                serde_json::json!({"status": 10}),
            ),
        ]);
        let events = shared.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            (events[0].session, events[0].tx, events[0].sequence),
            (4, 42, 1)
        );
        assert_eq!(
            (events[1].session, events[1].tx, events[1].sequence),
            (4, 0, 2)
        );
        assert_eq!(events[1].value["status"], 10);
    }
    #[test]
    fn serial_logs_retain_open_port_until_close_or_disconnect() {
        let shared = Shared {
            state: Mutex::new(Snapshot::default()),
            level: LogLevel::Error,
            events: Mutex::new(VecDeque::new()),
            telemetry_ms: AtomicU16::new(100),
            bone_ms: AtomicU16::new(25),
            bones_enabled: AtomicBool::new(false),
            events_overflow: AtomicBool::new(false),
            reset_listener: Mutex::new(None),
        };
        let update = |value| shared.rpc(vec![("SerialUpdateResponse".into(), 1, value)]);
        update(
            serde_json::json!({"closed":false,"device":{"port":"COM4","type":0},"log":"hello\n"}),
        );
        update(serde_json::json!({"closed":false,"device":null,"log":"continued\n"}));
        {
            let state = shared.state.lock().unwrap();
            assert_eq!(state.serial_device.as_ref().unwrap()["port"], "COM4");
            assert_eq!(state.serial_log, "hello\ncontinued\n");
        }
        update(serde_json::json!({"closed":true,"device":null}));
        assert!(shared.state.lock().unwrap().serial_device.is_none());
        update(serde_json::json!({"closed":false,"device":{"port":"COM5","type":1}}));
        {
            let mut state = shared.state.lock().unwrap();
            state.serial_device = None;
            state.serial_closed_locally = true;
        }
        update(
            serde_json::json!({"closed":false,"device":{"port":"COM5","type":1},"log":"late frame"}),
        );
        assert!(shared.state.lock().unwrap().serial_device.is_none());
        assert!(
            !shared
                .state
                .lock()
                .unwrap()
                .serial_log
                .contains("late frame")
        );
        shared.state.lock().unwrap().serial_closed_locally = false;
        update(serde_json::json!({"closed":false,"device":{"port":"COM6","type":0}}));
        assert_eq!(
            shared.state.lock().unwrap().serial_device.as_ref().unwrap()["port"],
            "COM6"
        );
        shared.disconnect("connection lost".into());
        assert!(shared.state.lock().unwrap().serial_device.is_none());
    }
    #[test]
    fn reset_feedback_survives_no_ui_polling_and_releases_state_lock() {
        let shared = Arc::new(Shared {
            state: Mutex::new(Snapshot {
                session: 8,
                ..Default::default()
            }),
            level: LogLevel::Error,
            events: Mutex::new(VecDeque::new()),
            telemetry_ms: AtomicU16::new(100),
            bone_ms: AtomicU16::new(25),
            bones_enabled: AtomicBool::new(false),
            events_overflow: AtomicBool::new(false),
            reset_listener: Mutex::new(None),
        });
        let recorded = Arc::new(Mutex::new((
            crate::sounds::Sequencer::default(),
            Vec::new(),
        )));
        let output = recorded.clone();
        let weak = Arc::downgrade(&shared);
        *shared.reset_listener.lock().unwrap() = Some(Arc::new(move |session, reset| {
            assert!(weak.upgrade().unwrap().state.try_lock().is_ok());
            let mut sound = output.lock().unwrap();
            let cues = sound
                .0
                .reset(session, reset.tx, reset.kind, reset.done, reset.progress_ms);
            sound.1.extend(cues);
        }));
        for kind in [1, 2] {
            for (progress_ms, done) in [
                (0, false),
                (1000, false),
                (1000, false),
                (2000, false),
                (3000, true),
                (3000, true),
            ] {
                shared.apply(vec![Update::Reset(ResetProgress {
                    tx: u32::from(kind),
                    kind,
                    done,
                    progress_ms,
                    duration_ms: 3000,
                })]);
            }
        }
        use crate::sounds::Cue;
        assert_eq!(
            recorded.lock().unwrap().1,
            vec![
                Cue::Initial(1),
                Cue::Tick(1, 1),
                Cue::Tick(1, 2),
                Cue::Finished(1),
                Cue::Initial(2),
                Cue::Tick(2, 1),
                Cue::Tick(2, 2),
                Cue::Finished(2)
            ]
        );
        assert!(shared.state.lock().unwrap().reset.as_ref().unwrap().done);
    }
}
