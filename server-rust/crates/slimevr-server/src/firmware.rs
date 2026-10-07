//! Original SolarXR firmware RPCs, bounded downloads, ESP OTA and native serial flashing.
use crate::{
    api::{protocol::rpc_frame, FrontendConfig, Request},
    receiver::Receiver,
    serial,
};
use futures_util::StreamExt;
use md5::{Digest, Md5};
use serialport::SerialPort;
use sha2::{Sha256, Sha512};
use solarxr_protocol::{
    datatypes as dt,
    rpc::{self, FirmwareUpdateStatus as S},
};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UdpSocket},
    sync::mpsc,
    task::JoinHandle,
};
const MAX_IMAGE: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Ota(u8),
    Serial(String),
}
#[derive(Clone, Debug)]
pub struct Part {
    pub url: String,
    pub digest: String,
    pub offset: u32,
}
impl Part {
    fn read(p: rpc::FirmwarePart<'_>) -> Result<Self, String> {
        let part = Self {
            url: p.url().ok_or("missing firmware URL")?.into(),
            digest: p.digest().ok_or("missing firmware digest")?.into(),
            offset: p.offset(),
        };
        let url = reqwest::Url::parse(&part.url).map_err(|_| "invalid firmware URL")?;
        if !matches!(url.scheme(), "https" | "http")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("firmware URL must be HTTP(S) without credentials".into());
        }
        verify_digest(&[], &part.digest)?;
        Ok(part)
    }
}
#[derive(Clone, Debug)]
enum Target {
    Ota {
        key: String,
        address: SocketAddr,
        session: u64,
    },
    Serial {
        port: serial::Port,
        manual: bool,
        ssid: String,
        password: String,
    },
}
#[derive(Debug)]
pub enum Event {
    CancelledSerial(u64),
    Progress {
        key: Key,
        token: u64,
        status: S,
        progress: i8,
    },
    Ready {
        key: Key,
        token: u64,
        files: tempfile::TempDir,
    },
    SerialFlashed {
        key: Key,
        token: u64,
    },
    Failed {
        key: Key,
        token: u64,
        status: S,
        error: String,
    },
}
struct Job {
    token: u64,
    target: Target,
    parts: Vec<Part>,
    task: Option<JoinHandle<()>>,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    files: Option<tempfile::TempDir>,
    status: S,
    progress: i8,
    since: u64,
    wait_session: bool,
    disconnected: bool,
}
type FirmwareEventResult = (Option<Vec<u8>>, Option<(String, String, String)>);
#[derive(Default)]
pub struct Controller {
    jobs: BTreeMap<Key, Job>,
    next: u64,
    cancelling: Option<u64>,
}
impl Drop for Controller {
    fn drop(&mut self) {
        for j in self.jobs.values() {
            if let Some(t) = &j.task {
                t.abort();
            }
        }
    }
}
impl Controller {
    pub fn serial_busy(&self) -> bool {
        self.cancelling.is_some()
            || self
                .jobs
                .iter()
                .any(|(key, j)| matches!(key, Key::Serial(_)) && !terminal(j.status))
    }
    pub fn queue(
        &mut self,
        r: rpc::FirmwareUpdateRequest<'_>,
        c: &FrontendConfig,
        receiver: &Receiver,
        ports: &[serial::Port],
        sender: &mpsc::Sender<Request>,
        at: u64,
    ) -> Result<Vec<u8>, String> {
        let (key, target, parts, wait) = match r.method_type() {
            rpc::FirmwareUpdateMethod::OTAFirmwareUpdate => {
                let r = r
                    .method_as_otafirmware_update()
                    .ok_or("missing OTA update")?;
                let id = r.device_id().ok_or("missing device ID")?.id();
                let device = c
                    .device_ids
                    .iter()
                    .find(|(_, v)| **v == id)
                    .and_then(|(k, _)| receiver.devices.get(k))
                    .ok_or("firmware device not found")?;
                if device.origin != crate::receiver::Origin::Udp {
                    return Err("OTA firmware updates require a Wi-Fi device".into());
                }
                let part = Part::read(r.firmware_part().ok_or("missing firmware image")?)?;
                (
                    Key::Ota(id),
                    Target::Ota {
                        key: device.key.clone(),
                        address: device.address,
                        session: device.session,
                    },
                    vec![part],
                    device.handshake.protocol_version <= 20,
                )
            }
            rpc::FirmwareUpdateMethod::SerialFirmwareUpdate => {
                if self.serial_busy() {
                    return Err("another serial firmware update is active".into());
                }
                let r = r
                    .method_as_serial_firmware_update()
                    .ok_or("missing serial update")?;
                let path = r
                    .device_id()
                    .and_then(|p| p.port())
                    .ok_or("missing serial port")?;
                let port = ports
                    .iter()
                    .find(|p| p.path == path)
                    .ok_or("firmware serial device not found")?
                    .clone();
                if port.kind() != rpc::SerialDeviceType::ESP_TRACKER {
                    return Err("ESP flashing is not supported for HID devices".into());
                }
                let ssid = r.ssid().ok_or("missing SSID")?.to_owned();
                let password = r.password().ok_or("missing password")?.to_owned();
                serial::wifi(&ssid, &password)?;
                let values = r.firmware_part().ok_or("missing firmware parts")?;
                if values.is_empty() || values.len() > 16 {
                    return Err("invalid firmware part count".into());
                }
                let parts = values
                    .iter()
                    .map(Part::read)
                    .collect::<Result<Vec<_>, _>>()?;
                validate_offsets(&parts)?;
                (
                    Key::Serial(path.into()),
                    Target::Serial {
                        port,
                        manual: r.needManualReboot(),
                        ssid,
                        password,
                    },
                    parts,
                    false,
                )
            }
            _ => return Err("unsupported firmware method".into()),
        };
        if self.jobs.get(&key).is_some_and(|j| !terminal(j.status)) {
            return Err("device is already updating".into());
        }
        self.jobs.retain(|_, j| !terminal(j.status));
        if self.jobs.len() >= 32 {
            return Err("firmware queue limit reached".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("firmware generation exhausted")?;
        let mut job = Job {
            token: self.next,
            target,
            parts,
            task: None,
            cancel: None,
            files: None,
            status: if wait {
                S::NEED_MANUAL_REBOOT
            } else {
                S::DOWNLOADING
            },
            progress: 0,
            since: at,
            wait_session: wait,
            disconnected: false,
        };
        if !wait {
            launch_download(&key, &mut job, sender);
        }
        let frame = frame(&key, job.status, 0);
        self.jobs.insert(key, job);
        Ok(frame)
    }
    pub fn event(
        &mut self,
        event: Event,
        serial: &serial::Controller,
        sender: &mpsc::Sender<Request>,
        at: u64,
    ) -> Result<FirmwareEventResult, String> {
        if let Event::CancelledSerial(token) = event {
            if self.cancelling == Some(token) {
                self.cancelling = None;
                serial.send(serial::Action::Resume)?;
            }
            return Ok((None, None));
        }
        let (key, token) = match &event {
            Event::Progress { key, token, .. }
            | Event::Ready { key, token, .. }
            | Event::SerialFlashed { key, token }
            | Event::Failed { key, token, .. } => (key.clone(), *token),
            Event::CancelledSerial(_) => unreachable!(),
        };
        let Some(job) = self
            .jobs
            .get_mut(&key)
            .filter(|j| j.token == token && !terminal(j.status))
        else {
            return Ok((None, None));
        };
        let mut provision = None;
        match event {
            Event::CancelledSerial(_) => unreachable!(),
            Event::Progress {
                status, progress, ..
            } => {
                job.status = status;
                job.progress = progress.clamp(0, 100);
            }
            Event::Ready { token, files, .. } => {
                job.files = Some(files);
                job.status = S::SYNCING_WITH_MCU;
                if let Err(e) = serial.send(serial::Action::Suspend { token }) {
                    job.status = S::ERROR_UNKNOWN;
                    job.files = None;
                    return Err(e);
                }
            }
            Event::SerialFlashed { .. } => {
                serial.send(serial::Action::Resume)?;
                job.files = None;
                if let Target::Serial {
                    port,
                    ssid,
                    password,
                    manual,
                } = &job.target
                {
                    if *manual {
                        job.status = S::NEED_MANUAL_REBOOT;
                        serial.send(serial::Action::Open {
                            port: Some(port.path.clone()),
                            auto: false,
                            include_hid: false,
                        })?;
                    } else {
                        job.status = S::REBOOTING;
                        provision = Some((port.path.clone(), ssid.clone(), password.clone()));
                    }
                }
            }
            Event::Failed { error, status, .. } => {
                job.status = status;
                job.files = None;
                if matches!(key, Key::Serial(_)) {
                    serial.send(serial::Action::Resume)?;
                }
                job.since = at;
                return Err(error);
            }
        }
        job.since = at;
        let response = frame(&key, job.status, job.progress);
        let _ = sender;
        Ok((Some(response), provision))
    }
    pub fn suspended(&mut self, token: u64, sender: &mpsc::Sender<Request>) {
        let Some((key, job)) = self.jobs.iter_mut().find(|(_, j)| {
            j.token == token && j.files.is_some() && j.status == S::SYNCING_WITH_MCU
        }) else {
            return;
        };
        let Target::Serial { port, .. } = &job.target else {
            return;
        };
        let key = key.clone();
        let path = port.path.clone();
        let manifest = job.files.as_ref().unwrap().path().join("manifest.json");
        let sender = sender.clone();
        let vid = port.vid;
        let pid = port.pid;
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        job.cancel = Some(cancel);
        job.task = Some(tokio::spawn(async move {
            let result =
                flash_child(&path, vid, pid, &manifest, &key, token, &sender, cancelled).await;
            let event = match result {
                Ok(()) => Event::SerialFlashed { key, token },
                Err(error) => Event::Failed {
                    key,
                    token,
                    status: S::ERROR_UPLOAD_FAILED,
                    error,
                },
            };
            let _ = sender.send(Request::Firmware(event)).await;
        }));
    }
    pub fn serial_event(
        &mut self,
        event: &serial::Event,
        at: u64,
    ) -> Option<(String, String, String)> {
        for job in self.jobs.values_mut() {
            let Target::Serial {
                port,
                ssid,
                password,
                ..
            } = &job.target
            else {
                continue;
            };
            if job.status != S::NEED_MANUAL_REBOOT {
                continue;
            }
            if matches!(event, serial::Event::Closed)
                || matches!(event,serial::Event::Ports(ports) if !ports.iter().any(|p|p.path==port.path))
            {
                job.disconnected = true;
            }
            if (job.disconnected
                && (matches!(event,serial::Event::Ports(ports) if ports.iter().any(|p|p.path==port.path))
                    || matches!(event,serial::Event::Log{server:false,log} if log.contains("SlimeVR")||log.contains("mac: "))))
                || matches!(event,serial::Event::Log{server:false,log}if log.contains("starting up..."))
            {
                job.status = S::REBOOTING;
                job.since = at;
                return Some((port.path.clone(), ssid.clone(), password.clone()));
            }
        }
        None
    }
    pub fn provisioning(
        &mut self,
        status: rpc::WifiProvisioningStatus,
        port: Option<&str>,
        at: u64,
    ) -> Option<Vec<u8>> {
        let key = Key::Serial(port?.into());
        let job = self.jobs.get_mut(&key)?;
        if terminal(job.status) {
            return None;
        }
        let new = match status {
            rpc::WifiProvisioningStatus::PROVISIONING => S::PROVISIONING,
            rpc::WifiProvisioningStatus::DONE => S::DONE,
            rpc::WifiProvisioningStatus::CONNECTION_ERROR
            | rpc::WifiProvisioningStatus::COULD_NOT_FIND_SERVER
            | rpc::WifiProvisioningStatus::NO_SERIAL_DEVICE_FOUND
            | rpc::WifiProvisioningStatus::NO_SERIAL_LOGS_ERROR => S::ERROR_PROVISIONING_FAILED,
            _ => return None,
        };
        job.status = new;
        job.since = at;
        if terminal(new) {
            if let Target::Serial { password, .. } = &mut job.target {
                password.clear();
            }
        }
        Some(frame(&key, new, job.progress))
    }
    pub fn tick(
        &mut self,
        receiver: &Receiver,
        sender: &mpsc::Sender<Request>,
        at: u64,
    ) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for (key, job) in &mut self.jobs {
            if terminal(job.status) {
                continue;
            }
            if let Target::Ota {
                key: device,
                session,
                ..
            } = &job.target
            {
                if let Some(d) = receiver
                    .devices
                    .get(device)
                    .filter(|d| !d.transport_timed_out && d.session > *session)
                {
                    if job.wait_session {
                        job.wait_session = false;
                        job.target = Target::Ota {
                            key: d.key.clone(),
                            address: d.address,
                            session: d.session,
                        };
                        job.status = S::DOWNLOADING;
                        job.since = at;
                        launch_download(key, job, sender);
                        frames.push(frame(key, job.status, 0));
                    } else if job.status == S::REBOOTING {
                        job.status = S::DONE;
                        frames.push(frame(key, S::DONE, 100));
                    }
                }
            }
            if matches!(
                job.status,
                S::REBOOTING | S::PROVISIONING | S::NEED_MANUAL_REBOOT
            ) && at.saturating_sub(job.since) > 120000
            {
                job.status = S::ERROR_TIMEOUT;
                frames.push(frame(key, job.status, 0));
            }
        }
        frames
    }
    pub fn cancel(&mut self, sender: &mpsc::Sender<Request>) -> Vec<Vec<u8>> {
        let frames = self
            .jobs
            .iter()
            .filter(|(_, j)| !terminal(j.status))
            .map(|(k, _)| frame(k, S::ERROR_UNKNOWN, 0))
            .collect();
        let had_serial = self.serial_busy();
        let mut waiting = vec![];
        for (_, mut job) in std::mem::take(&mut self.jobs) {
            if let Some(cancel) = job.cancel.take() {
                let _ = cancel.send(());
            } else if let Some(task) = &job.task {
                task.abort();
            }
            if let Some(task) = job.task.take() {
                waiting.push(task);
            }
        }
        if had_serial && self.cancelling.is_none() {
            self.next = self.next.saturating_add(1);
            let token = self.next;
            self.cancelling = Some(token);
            let sender = sender.clone();
            tokio::spawn(async move {
                for task in waiting {
                    let _ = task.await;
                }
                let _ = sender
                    .send(Request::Firmware(Event::CancelledSerial(token)))
                    .await;
            });
        }
        frames
    }
    pub fn status_frame(&self, key: &Key) -> Option<Vec<u8>> {
        self.jobs.get(key).map(|j| frame(key, j.status, j.progress))
    }
    pub fn failed_frame(&self, event: &Event) -> Option<Vec<u8>> {
        let key = match event {
            Event::Failed { key, .. } => key,
            _ => return None,
        };
        self.jobs.get(key).map(|j| frame(key, j.status, j.progress))
    }
}
fn terminal(status: S) -> bool {
    status == S::DONE || status.0 >= S::ERROR_DEVICE_NOT_FOUND.0
}
fn validate_offsets(parts: &[Part]) -> Result<(), String> {
    if parts
        .iter()
        .any(|p| p.offset >= MAX_IMAGE as u32 || p.offset % 4096 != 0)
    {
        return Err("invalid or unaligned flash offset".into());
    }
    let mut offsets = parts.iter().map(|p| p.offset).collect::<Vec<_>>();
    offsets.sort_unstable();
    if offsets.windows(2).any(|p| p[0] == p[1]) {
        return Err("duplicate flash offsets".into());
    }
    Ok(())
}
fn launch_download(key: &Key, job: &mut Job, sender: &mpsc::Sender<Request>) {
    let key = key.clone();
    let token = job.token;
    let target = job.target.clone();
    let parts = job.parts.clone();
    let sender = sender.clone();
    job.task = Some(tokio::spawn(async move {
        let download = tokio::time::timeout(Duration::from_secs(30), download_parts(&parts)).await;
        let images = match download {
            Ok(Ok(images)) => images,
            Ok(Err(error)) => {
                let _ = sender
                    .send(Request::Firmware(Event::Failed {
                        key,
                        token,
                        status: S::ERROR_DOWNLOAD_FAILED,
                        error,
                    }))
                    .await;
                return;
            }
            Err(_) => {
                let _ = sender
                    .send(Request::Firmware(Event::Failed {
                        key,
                        token,
                        status: S::ERROR_DOWNLOAD_FAILED,
                        error: "firmware download timed out".into(),
                    }))
                    .await;
                return;
            }
        };
        match target {
            Target::Ota { address, .. } => {
                let bytes = &images[0].1;
                let result = tokio::time::timeout(
                    Duration::from_secs(120),
                    ota(address.ip(), 8266, bytes, |status, progress| {
                        let sender = sender.clone();
                        let key = key.clone();
                        async move {
                            sender
                                .send(Request::Firmware(Event::Progress {
                                    key,
                                    token,
                                    status,
                                    progress,
                                }))
                                .await
                                .map_err(|_| "server stopped".to_owned())
                        }
                    }),
                )
                .await;
                match result {
                    Ok(Ok(())) => {
                        let _ = sender
                            .send(Request::Firmware(Event::Progress {
                                key,
                                token,
                                status: S::REBOOTING,
                                progress: 100,
                            }))
                            .await;
                    }
                    result => {
                        let (status, error) = match result {
                            Ok(Err(e)) => e,
                            Err(_) => (S::ERROR_TIMEOUT, "OTA update timed out".into()),
                            _ => unreachable!(),
                        };
                        let _ = sender
                            .send(Request::Firmware(Event::Failed {
                                key,
                                token,
                                status,
                                error,
                            }))
                            .await;
                    }
                }
            }
            Target::Serial { .. } => {
                let files = write_images(&images);
                let event = match files {
                    Ok(files) => Event::Ready { key, token, files },
                    Err(error) => Event::Failed {
                        key,
                        token,
                        status: S::ERROR_DOWNLOAD_FAILED,
                        error,
                    },
                };
                let _ = sender.send(Request::Firmware(event)).await;
            }
        }
    }));
}
pub fn verify_digest(bytes: &[u8], digest: &str) -> Result<bool, String> {
    let (algorithm, expected) = digest
        .split_once(':')
        .ok_or("expected firmware digest algorithm:hash")?;
    let algorithm = algorithm.to_ascii_uppercase().replace('-', "");
    let actual = match algorithm.as_str() {
        "SHA256" => format!("{:x}", Sha256::digest(bytes)),
        "SHA512" => format!("{:x}", Sha512::digest(bytes)),
        "MD5" => format!("{:x}", Md5::digest(bytes)),
        _ => return Err("unsupported firmware digest algorithm".into()),
    };
    if expected.len() != actual.len() || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid firmware digest".into());
    }
    Ok(actual.eq_ignore_ascii_case(expected))
}
async fn download_parts(parts: &[Part]) -> Result<Vec<(u32, Vec<u8>)>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut images = Vec::new();
    let mut total = 0;
    for p in parts {
        let response = client
            .get(&p.url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_IMAGE as u64)
        {
            return Err("firmware image exceeds limit".into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            total += chunk.len();
            if total > MAX_IMAGE {
                return Err("firmware parts exceed limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() || !verify_digest(&bytes, &p.digest)? {
            return Err("firmware checksum verification failed".into());
        }
        images.push((p.offset, bytes));
    }
    images.sort_by_key(|(offset, _)| *offset);
    if images
        .windows(2)
        .any(|p| p[0].0 as usize + p[0].1.len() > p[1].0 as usize)
        || images
            .iter()
            .any(|(o, b)| *o as usize + b.len() > MAX_IMAGE)
    {
        return Err("overlapping or out-of-range flash parts".into());
    }
    Ok(images)
}
#[derive(serde::Serialize, serde::Deserialize)]
struct FlashImage {
    offset: u32,
    path: PathBuf,
}
fn write_images(images: &[(u32, Vec<u8>)]) -> Result<tempfile::TempDir, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut manifest = Vec::new();
    for (i, (offset, bytes)) in images.iter().enumerate() {
        let path = dir.path().join(format!("part-{i}.bin"));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        manifest.push(FlashImage {
            offset: *offset,
            path,
        });
    }
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(dir)
}
#[allow(clippy::too_many_arguments)]
async fn flash_child(
    port: &str,
    vid: u16,
    pid: u16,
    manifest: &std::path::Path,
    key: &Key,
    token: u64,
    sender: &mpsc::Sender<Request>,
    cancelled: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = tokio::process::Command::new(executable);
    command
        .arg("flash-worker")
        .arg("--port")
        .arg(port)
        .arg("--vid")
        .arg(vid.to_string())
        .arg("--pid")
        .arg(pid.to_string())
        .arg("--manifest")
        .arg(manifest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut text = vec![];
        let _ = stderr.take(8192).read_to_end(&mut text).await;
        text
    });
    let result = {
        let work = async {
            while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
                if let Ok(progress) = line.parse::<i8>() {
                    sender
                        .send(Request::Firmware(Event::Progress {
                            key: key.clone(),
                            token,
                            status: S::UPLOADING,
                            progress,
                        }))
                        .await
                        .map_err(|_| "server stopped")?;
                }
            }
            child.wait().await.map_err(|e| e.to_string())
        };
        tokio::select! {result=work=>result,_=cancelled=>Err("serial firmware update cancelled".into()),_=tokio::time::sleep(Duration::from_secs(120))=>Err("serial firmware update timed out".into())}
    };
    if result.is_err() {
        let _ = child.kill().await;
    }
    let stderr = errors.await.unwrap_or_default();
    let status = result?;
    if !status.success() {
        return Err(format!(
            "native ESP flashing failed: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    Ok(())
}
/// Invoked only in an owned child. Cancellation kills the process and releases the port.
pub fn flash_worker(
    port: &str,
    vid: u16,
    pid: u16,
    manifest: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use espflash::{
        connection::{Connection, ResetAfterOperation, ResetBeforeOperation},
        flasher::Flasher,
        image_format::Segment,
        target::ProgressCallbacks,
    };
    use std::{borrow::Cow, io::Write};
    let values: Vec<FlashImage> = serde_json::from_slice(&std::fs::read(manifest)?)?;
    if values.is_empty() || values.len() > 16 {
        return Err("invalid flash manifest".into());
    }
    let images = values
        .iter()
        .map(|p| std::fs::read(&p.path))
        .collect::<Result<Vec<_>, _>>()?;
    if images.iter().map(Vec::len).sum::<usize>() > MAX_IMAGE {
        return Err("oversized flash image".into());
    }
    let mut serial = serialport::new(port, 115200)
        .timeout(Duration::from_secs(3))
        .open_native()?;
    let parts = values
        .iter()
        .zip(&images)
        .map(|(p, b)| (p.offset, b.as_slice()))
        .collect::<Vec<_>>();
    if crate::serial::esp8266::try_flash(&mut serial, &parts)? {
        return Ok(());
    }
    let _ = serial.clear(serialport::ClearBuffer::All);
    let info = serialport::UsbPortInfo {
        vid,
        pid,
        serial_number: None,
        manufacturer: None,
        product: None,
    };
    let connection = Connection::new(
        serial,
        info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        115200,
    );
    let mut flasher = Flasher::connect(connection, true, true, false, None, None)?;
    struct Progress {
        done: usize,
        segment: usize,
        total: usize,
        weight: usize,
        weights: BTreeMap<u32, usize>,
    }
    impl ProgressCallbacks for Progress {
        fn init(&mut self, addr: u32, total: usize) {
            self.segment = total;
            self.weight = self.weights[&addr];
        }
        fn update(&mut self, current: usize) {
            println!(
                "{}",
                ((self.done + current.saturating_mul(self.weight) / self.segment.max(1))
                    .saturating_mul(100)
                    / self.total.max(1))
                .min(100)
            );
            let _ = std::io::stdout().flush();
        }
        fn verifying(&mut self) {}
        fn finish(&mut self, _: bool) {
            self.done += self.weight;
        }
    }
    let mut progress = Progress {
        done: 0,
        segment: 0,
        total: images.iter().map(Vec::len).sum(),
        weight: 0,
        weights: values
            .iter()
            .zip(&images)
            .map(|(p, b)| (p.offset, b.len()))
            .collect(),
    };
    let segments = values
        .iter()
        .zip(&images)
        .map(|(p, data)| Segment {
            addr: p.offset,
            data: Cow::Borrowed(data),
        })
        .collect::<Vec<_>>();
    flasher.write_bins_to_flash(&segments, &mut progress)?;
    Ok(())
}
/// Arduino/ESP OTA invite, challenge-response and four-byte chunk acknowledgements.
pub async fn ota<F, Fut>(
    ip: IpAddr,
    port: u16,
    bytes: &[u8],
    mut progress: F,
) -> Result<(), (S, String)>
where
    F: FnMut(S, i8) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let bind = SocketAddr::new(
        if ip.is_ipv4() {
            "0.0.0.0".parse().unwrap()
        } else {
            "::".parse().unwrap()
        },
        0,
    );
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| (S::ERROR_UPLOAD_FAILED, e.to_string()))?;
    let udp = UdpSocket::bind(bind)
        .await
        .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
    udp.connect(SocketAddr::new(ip, port))
        .await
        .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
    progress(S::AUTHENTICATING, 0)
        .await
        .map_err(|e| (S::ERROR_UNKNOWN, e))?;
    let invite = format!(
        "0 {} {} {:x}\n",
        listener.local_addr().unwrap().port(),
        bytes.len(),
        Md5::digest(bytes)
    );
    udp.send(invite.as_bytes())
        .await
        .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
    let mut buf = [0u8; 256];
    let n = tokio::time::timeout(Duration::from_secs(10), udp.recv(&mut buf))
        .await
        .map_err(|_| {
            (
                S::ERROR_AUTHENTICATION_FAILED,
                "OTA invitation timed out".into(),
            )
        })?
        .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
    let text = std::str::from_utf8(&buf[..n])
        .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
    if text != "OK" {
        let nonce = text
            .strip_prefix("AUTH ")
            .filter(|s| !s.is_empty() && !s.contains(char::is_whitespace))
            .ok_or((
                S::ERROR_AUTHENTICATION_FAILED,
                "invalid OTA challenge".into(),
            ))?;
        let cnonce = format!(
            "{:x}",
            Md5::digest(format!(
                "{:?}-{}",
                std::time::SystemTime::now(),
                listener.local_addr().unwrap()
            ))
        );
        let password = format!("{:x}", Md5::digest(b"SlimeVR-OTA"));
        let digest = format!("{:x}", Md5::digest(format!("{password}:{nonce}:{cnonce}")));
        udp.send(format!("200 {cnonce} {digest}\n").as_bytes())
            .await
            .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
        let n = tokio::time::timeout(Duration::from_secs(10), udp.recv(&mut buf))
            .await
            .map_err(|_| {
                (
                    S::ERROR_AUTHENTICATION_FAILED,
                    "OTA authentication timed out".into(),
                )
            })?
            .map_err(|e| (S::ERROR_AUTHENTICATION_FAILED, e.to_string()))?;
        if &buf[..n] != b"OK" {
            return Err((
                S::ERROR_AUTHENTICATION_FAILED,
                "OTA authentication refused".into(),
            ));
        }
    }
    let (mut socket, peer) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .map_err(|_| {
            (
                S::ERROR_UPLOAD_FAILED,
                "device did not connect for OTA".into(),
            )
        })?
        .map_err(|e| (S::ERROR_UPLOAD_FAILED, e.to_string()))?;
    if peer.ip() != ip {
        return Err((S::ERROR_UPLOAD_FAILED, "unexpected OTA peer".into()));
    }
    let mut sent = 0;
    for chunk in bytes.chunks(2048) {
        tokio::time::timeout(Duration::from_secs(10), async {
            socket.write_all(chunk).await?;
            socket.flush().await?;
            let mut ack = [0u8; 4];
            socket.read_exact(&mut ack).await?;
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(|_| {
            (
                S::ERROR_UPLOAD_FAILED,
                "OTA chunk acknowledgement timed out".into(),
            )
        })?
        .map_err(|e| (S::ERROR_UPLOAD_FAILED, e.to_string()))?;
        sent += chunk.len();
        progress(S::UPLOADING, (sent * 100 / bytes.len().max(1)) as i8)
            .await
            .map_err(|e| (S::ERROR_UNKNOWN, e))?;
    }
    let mut result = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        (&mut socket).take(4096).read_to_end(&mut result),
    )
    .await
    .map_err(|_| (S::ERROR_UPLOAD_FAILED, "OTA result timed out".into()))?
    .map_err(|e| (S::ERROR_UPLOAD_FAILED, e.to_string()))?;
    if !result.windows(2).any(|v| v == b"OK") {
        return Err((
            S::ERROR_UPLOAD_FAILED,
            "device rejected uploaded firmware".into(),
        ));
    }
    Ok(())
}
pub fn frame(key: &Key, status: S, progress: i8) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::FirmwareUpdateStatusResponse, 0, |f| {
        let (kind, id) = match key {
            Key::Ota(id) => {
                let id = dt::DeviceId::new(*id);
                (
                    rpc::FirmwareUpdateDeviceId::solarxr_protocol_datatypes_DeviceIdTable,
                    dt::DeviceIdTable::create(f, &dt::DeviceIdTableArgs { id: Some(&id) })
                        .as_union_value(),
                )
            }
            Key::Serial(port) => {
                let port = f.create_string(port);
                (
                    rpc::FirmwareUpdateDeviceId::SerialDevicePort,
                    rpc::SerialDevicePort::create(
                        f,
                        &rpc::SerialDevicePortArgs { port: Some(port) },
                    )
                    .as_union_value(),
                )
            }
        };
        rpc::FirmwareUpdateStatusResponse::create(
            f,
            &rpc::FirmwareUpdateStatusResponseArgs {
                device_id_type: kind,
                device_id: Some(id),
                status,
                progress,
            },
        )
        .as_union_value()
    })
}
