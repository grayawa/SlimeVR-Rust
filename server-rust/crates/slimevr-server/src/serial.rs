//! Owned serial worker: enumeration, hotplug, firmware text commands and bounded logging.
pub mod esp8266;
pub mod provisioning;
use crate::api::{protocol::rpc_frame, Request};
use solarxr_protocol::{flatbuffers as fb, rpc};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub path: String,
    pub name: String,
    pub vid: u16,
    pub pid: u16,
}
impl Port {
    pub fn kind(&self) -> rpc::SerialDeviceType {
        if receiver(self.vid, self.pid) {
            rpc::SerialDeviceType::HID_RECEIVER
        } else if (self.vid, self.pid) == (0x1209, 0x7692) {
            rpc::SerialDeviceType::HID_TRACKER
        } else {
            rpc::SerialDeviceType::ESP_TRACKER
        }
    }
}
pub fn receiver(vid: u16, pid: u16) -> bool {
    (vid == 0x1209 && pid == 0x7690) || (vid == 0x4e76 && (pid & 0xfff0) == 0xd2d0)
}
pub fn supported(vid: u16, pid: u16) -> bool {
    receiver(vid, pid)
        || (vid, pid) == (0x1209, 0x7692)
        || matches!(
            (vid, pid),
            (0x1a86, 0x7522 | 0x7523 | 0x5523 | 0x55d3 | 0x55d4)
                | (0x10c4, 0xea60)
                | (0x303a, 0x1001 | 0x0002)
                | (0x0403, 0x6001)
        )
}
pub fn enumerate(extra: &[String]) -> Result<Vec<Port>, String> {
    let mut ports = serialport::available_ports()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|p| {
            let serialport::SerialPortType::UsbPort(u) = p.port_type else {
                return None;
            };
            supported(u.vid, u.pid).then(|| Port {
                path: p.port_name,
                name: u
                    .product
                    .or(u.manufacturer)
                    .unwrap_or_else(|| "SlimeVR serial device".into()),
                vid: u.vid,
                pid: u.pid,
            })
        })
        .collect::<Vec<_>>();
    for path in extra {
        if !ports.iter().any(|p| p.path == *path) {
            ports.push(Port {
                path: path.clone(),
                name: "Explicit serial port".into(),
                vid: 0,
                pid: 0,
            });
        }
    }
    ports.sort_by(|a, b| a.path.cmp(&b.path));
    ports.truncate(256);
    Ok(ports)
}
#[derive(Debug)]
pub enum Event {
    Ports(Vec<Port>),
    Connected(Port),
    Closed,
    Suspended(u64),
    Log { log: String, server: bool },
    Error(String),
}
#[derive(Debug)]
pub enum Action {
    Suspend {
        token: u64,
    },
    Resume,
    Open {
        port: Option<String>,
        auto: bool,
        include_hid: bool,
    },
    Close,
    Command(String),
    Wifi {
        ssid: String,
        password: String,
    },
}
pub fn command(text: &str) -> Result<Vec<u8>, String> {
    if text.is_empty() || text.len() > 4096 || text.contains(['\r', '\n', '\0']) {
        return Err("invalid serial command".into());
    }
    Ok(format!("{text}\n").into_bytes())
}
pub fn wifi(ssid: &str, password: &str) -> Result<(Vec<u8>, String), String> {
    if ssid.is_empty()
        || ssid.len() > 32
        || password.len() > 64
        || ssid.contains(['\r', '\n', '\0', '"'])
        || password.contains(['\r', '\n', '\0', '"'])
    {
        return Err(
            "Wi-Fi credentials cannot be represented by the firmware serial protocol".into(),
        );
    }
    Ok((
        format!("SET WIFI \"{ssid}\" \"{password}\"\n").into_bytes(),
        format!(
            "-> SET WIFI \"{ssid}\" \"{}\"\n",
            "*".repeat(password.chars().count())
        ),
    ))
}
pub struct Controller {
    pub ports: Vec<Port>,
    pub current: Option<Port>,
    sender: mpsc::SyncSender<Action>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Controller {
    pub fn start(
        events: tokio::sync::mpsc::Sender<Request>,
        extra: Vec<String>,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("slimevr-serial".into())
            .spawn(move || worker(receiver, events, extra, flag))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            ports: Vec::new(),
            current: None,
            sender,
            stop,
            thread: Some(thread),
        })
    }
    pub fn send(&self, action: Action) -> Result<(), String> {
        self.sender
            .try_send(action)
            .map_err(|_| "serial command queue is full or stopped".into())
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn worker(
    actions: mpsc::Receiver<Action>,
    events: tokio::sync::mpsc::Sender<Request>,
    extra: Vec<String>,
    stop: Arc<AtomicBool>,
) {
    let mut port: Option<Box<dyn serialport::SerialPort>> = None;
    let mut current: Option<Port> = None;
    let mut known = Vec::new();
    let mut scan = Instant::now() - Duration::from_secs(4);
    let mut pending = VecDeque::new();
    let mut line = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut secret = String::new();
    let mut suspended = false;
    while !stop.load(Ordering::Relaxed) {
        while let Some(event) = pending.pop_front() {
            match events.try_send(Request::Serial(event)) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(Request::Serial(event))) => {
                    pending.push_front(event);
                    break;
                }
                Err(_) => return,
            }
        }
        if pending.len() > 64 {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        if scan.elapsed() >= Duration::from_secs(3) {
            scan = Instant::now();
            match enumerate(&extra) {
                Ok(found) => {
                    if found != known {
                        if current.as_ref().is_some_and(|c| !found.contains(c)) {
                            port = None;
                            current = None;
                            pending.push_back(Event::Closed);
                        }
                        known = found;
                        pending.push_back(Event::Ports(known.clone()));
                    }
                }
                Err(e) => pending.push_back(Event::Error(e)),
            }
        }
        let action = if port.is_some() {
            actions.try_recv().ok()
        } else {
            match actions.recv_timeout(Duration::from_millis(50)) {
                Ok(a) => Some(a),
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(_) => None,
            }
        };
        if let Some(action) = action {
            if suspended && !matches!(action, Action::Resume | Action::Suspend { .. }) {
                pending.push_back(Event::Error(
                    "serial port is reserved for firmware flashing".into(),
                ));
                continue;
            }
            match action {
                Action::Suspend { token } => {
                    port = None;
                    current = None;
                    line.clear();
                    suspended = true;
                    pending.push_back(Event::Suspended(token));
                }
                Action::Resume => suspended = false,
                Action::Close => {
                    port = None;
                    current = None;
                    line.clear();
                    pending.push_back(Event::Closed);
                }
                Action::Open {
                    port: requested,
                    auto,
                    include_hid,
                } => {
                    let found = known
                        .iter()
                        .find(|p| {
                            (auto
                                && (include_hid || p.kind() == rpc::SerialDeviceType::ESP_TRACKER))
                                || (!auto && requested.as_ref() == Some(&p.path))
                        })
                        .cloned();
                    if let Some(found) = found {
                        if current.as_ref() == Some(&found) {
                            pending.push_back(Event::Connected(found));
                            continue;
                        }
                        port = None;
                        current = None;
                        line.clear();
                        match serialport::new(&found.path, 115200)
                            .timeout(Duration::from_millis(50))
                            .open()
                        {
                            Ok(mut opened) => {
                                let _ = opened.write_request_to_send(false);
                                let _ = opened.write_data_terminal_ready(false);
                                current = Some(found.clone());
                                port = Some(opened);
                                pending.push_back(Event::Connected(found));
                            }
                            Err(e) => {
                                pending.push_back(Event::Error(format!(
                                    "Unable to open serial port: {e}"
                                )));
                                pending.push_back(Event::Closed);
                            }
                        }
                    } else {
                        pending.push_back(Event::Error(
                            "requested serial device is not available".into(),
                        ));
                        pending.push_back(Event::Closed);
                    }
                }
                action => {
                    let data = match action {
                        Action::Command(text) => {
                            if text
                                .trim_start()
                                .to_ascii_uppercase()
                                .starts_with("SET WIFI ")
                            {
                                let fields: Vec<_> = text.split('"').collect();
                                if let Some(password) = fields.get(3) {
                                    secret = (*password).into();
                                }
                            }
                            command(&text)
                                .map(|bytes| (bytes, redact(&format!("-> {text}\n"), &secret)))
                        }
                        Action::Wifi { ssid, password } => {
                            let data = wifi(&ssid, &password);
                            if data.is_ok() {
                                secret = password;
                            }
                            data
                        }
                        _ => unreachable!(),
                    };
                    match (data, port.as_mut()) {
                        (Ok((bytes, log)), Some(port)) => {
                            match port.write_all(&bytes).and_then(|_| port.flush()) {
                                Ok(()) => pending.push_back(Event::Log { log, server: true }),
                                Err(e) => {
                                    pending.push_back(Event::Error(format!(
                                        "Serial write failed: {e}"
                                    )));
                                    port.clear(serialport::ClearBuffer::Output).ok();
                                }
                            }
                        }
                        (Err(e), _) => pending.push_back(Event::Error(e)),
                        (_, None) => {
                            pending.push_back(Event::Error("serial device is not connected".into()))
                        }
                    }
                }
            }
        }
        if let Some(opened) = &mut port {
            match opened.read(&mut buffer) {
                Ok(0) => {}
                Ok(n) => {
                    for b in &buffer[..n] {
                        line.push(*b);
                        if *b == b'\n' || line.len() >= 4096 {
                            pending.push_back(Event::Log {
                                log: redact(&String::from_utf8_lossy(&line), &secret),
                                server: false,
                            });
                            line.clear();
                        }
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => {
                    port = None;
                    current = None;
                    line.clear();
                    pending.push_back(Event::Error(format!("Serial device disconnected: {e}")));
                    pending.push_back(Event::Closed);
                }
            }
        }
    }
}
pub fn device<'a>(
    f: &mut fb::FlatBufferBuilder<'a>,
    port: &Port,
) -> fb::WIPOffset<rpc::SerialDevice<'a>> {
    let path = f.create_string(&port.path);
    let name = f.create_string(&port.name);
    rpc::SerialDevice::create(
        f,
        &rpc::SerialDeviceArgs {
            port: Some(path),
            name: Some(name),
            type_: port.kind(),
        },
    )
}
pub fn list(tx: u32, ports: &[Port]) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::SerialDevicesResponse, tx, |f| {
        let values = ports.iter().map(|p| device(f, p)).collect::<Vec<_>>();
        let devices = f.create_vector(&values);
        rpc::SerialDevicesResponse::create(
            f,
            &rpc::SerialDevicesResponseArgs {
                devices: Some(devices),
            },
        )
        .as_union_value()
    })
}
pub fn update(port: Option<&Port>, log: Option<&str>, closed: bool) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::SerialUpdateResponse, 0, |f| {
        let device = port.map(|p| device(f, p));
        let log = log.map(|l| f.create_string(l));
        rpc::SerialUpdateResponse::create(
            f,
            &rpc::SerialUpdateResponseArgs {
                device,
                log,
                closed,
            },
        )
        .as_union_value()
    })
}

pub fn redact(log: &str, secret: &str) -> String {
    if secret.is_empty() {
        log.into()
    } else {
        log.replace(secret, &"*".repeat(secret.chars().count()))
    }
}
