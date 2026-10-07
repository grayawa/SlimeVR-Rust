//! Native HID reports are delivered to the same input owner as UDP, with bounded backpressure.
use crate::{api::Request, serial};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Report {
        path: String,
        session: u64,
        bytes: Vec<u8>,
    },
    Closed {
        path: String,
        session: u64,
    },
    Error {
        message: String,
    },
}
pub struct Controller {
    pub direct: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Controller {
    pub fn start(sender: tokio::sync::mpsc::Sender<Request>, direct: bool) -> Result<Self, String> {
        let direct = Arc::new(AtomicBool::new(direct));
        let stop = Arc::new(AtomicBool::new(false));
        let (setting, flag) = (direct.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("slimevr-hid".into())
            .spawn(move || worker(sender, setting, flag))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            direct,
            stop,
            thread: Some(thread),
        })
    }
    pub fn set_direct(&self, direct: bool) {
        self.direct.store(direct, Ordering::Relaxed);
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
struct Open {
    device: hidapi::HidDevice,
    session: u64,
    last: Instant,
}
fn worker(
    sender: tokio::sync::mpsc::Sender<Request>,
    direct: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    let mut api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            let _ = sender.try_send(Request::Hid(Event::Error {
                message: e.to_string(),
            }));
            return;
        }
    };
    let mut opened: BTreeMap<String, Open> = BTreeMap::new();
    let mut next = 0u64;
    let mut scan = Instant::now() - Duration::from_secs(2);
    let mut pending = VecDeque::new();
    let mut buffer = [0; 1024];
    while !stop.load(Ordering::Relaxed) {
        while let Some(event) = pending.pop_front() {
            match sender.try_send(Request::Hid(event)) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(Request::Hid(event))) => {
                    pending.push_front(event);
                    break;
                }
                Err(_) => return,
            }
        }
        if pending.len() > 64 {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        if scan.elapsed() >= Duration::from_secs(1) {
            scan = Instant::now();
            if let Err(e) = api.refresh_devices() {
                pending.push_back(Event::Error {
                    message: e.to_string(),
                });
            } else {
                let found = api
                    .device_list()
                    .filter(|d| {
                        serial::receiver(d.vendor_id(), d.product_id())
                            || (direct.load(Ordering::Relaxed)
                                && (d.vendor_id(), d.product_id()) == (0x1209, 0x7692))
                    })
                    .take(64)
                    .map(|d| (d.path().to_string_lossy().into_owned(), d.path().to_owned()))
                    .collect::<BTreeMap<_, _>>();
                let removed = opened
                    .iter()
                    .filter(|(path, open)| {
                        !found.contains_key(*path) || open.last.elapsed() > Duration::from_secs(5)
                    })
                    .map(|(p, o)| (p.clone(), o.session))
                    .collect::<Vec<_>>();
                for (path, session) in removed {
                    opened.remove(&path);
                    pending.push_back(Event::Closed { path, session });
                }
                for (path, cpath) in found {
                    if let std::collections::btree_map::Entry::Vacant(entry) = opened.entry(path) {
                        match api.open_path(&cpath) {
                            Ok(device) => {
                                next = next.saturating_add(1);
                                entry.insert(Open {
                                    device,
                                    session: next,
                                    last: Instant::now(),
                                });
                            }
                            Err(e) => pending.push_back(Event::Error {
                                message: format!("HID open failed: {e}"),
                            }),
                        }
                    }
                }
            }
        }
        let mut received = false;
        let mut failed = Vec::new();
        for (path, open) in &mut opened {
            match open.device.read_timeout(&mut buffer, 0) {
                Ok(0) => {}
                Ok(n) => {
                    open.last = Instant::now();
                    received = true;
                    let data = if n == 65 && buffer[0] == 0 {
                        &buffer[1..n]
                    } else {
                        &buffer[..n]
                    };
                    if data.len().is_multiple_of(16) {
                        pending.push_back(Event::Report {
                            path: path.clone(),
                            session: open.session,
                            bytes: data.to_vec(),
                        });
                    }
                }
                Err(e) => {
                    pending.push_back(Event::Error {
                        message: format!("HID read failed: {e}"),
                    });
                    failed.push((path.clone(), open.session));
                }
            }
        }
        for (path, session) in failed {
            opened.remove(&path);
            pending.push_back(Event::Closed { path, session });
        }
        if !received {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
/// Fixed-point and exponential-map packets use the original firmware axis convention.
pub fn rotation(data: &[u8; 16]) -> Option<slimevr_core::Quaternion> {
    use slimevr_core::Quaternion as Q;
    let q = match data[0] {
        1 | 4 => {
            let mut v = [0.0; 4];
            for (i, n) in v.iter_mut().enumerate() {
                *n = i16::from_le_bytes(data[2 + i * 2..4 + i * 2].try_into().unwrap()) as f32
                    / 32768.0;
            }
            Q::new(v[3], v[0], v[1], v[2])
        }
        2 | 7 => {
            let bits = u32::from_le_bytes(data[5..9].try_into().unwrap());
            let v = [
                (bits & 1023) as f32 / 1024.0 * 2.0 - 1.0,
                ((bits >> 10) & 2047) as f32 / 2048.0 * 2.0 - 1.0,
                ((bits >> 21) & 2047) as f32 / 2048.0 * 2.0 - 1.0,
            ];
            let d = v.iter().map(|v| v * v).sum::<f32>();
            let inv = 1.0 / (d + 1e-6).sqrt();
            let a = std::f32::consts::FRAC_PI_2 * d * inv;
            let k = a.sin() * inv;
            Q::new(a.cos(), k * v[0], k * v[1], k * v[2])
        }
        _ => return None,
    };
    Some(
        Q::new(
            std::f32::consts::FRAC_1_SQRT_2,
            -std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            0.0,
        ) * q,
    )
    .filter(|q| q.is_rotation())
}
pub fn vector(data: &[u8; 16], offset: usize, scale: f32) -> slimevr_core::Vector3 {
    let read = |i| i16::from_le_bytes(data[i..i + 2].try_into().unwrap()) as f32 * scale;
    slimevr_core::Vector3::new(read(offset), read(offset + 2), read(offset + 4))
}
