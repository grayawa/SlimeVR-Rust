//! Exactly one cue per reset stage, independent of duplicate packets or redraws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Initial(u8),
    Tick(u8, u8),
    Finished(u8),
    Pause(bool),
    Tap(u8),
}
#[derive(Default)]
pub struct Sequencer {
    current: Option<(u64, u32, u8)>,
    last_second: i32,
    finished: bool,
    pause: Option<bool>,
    tap: u8,
}
impl Sequencer {
    pub fn reset(
        &mut self,
        session: u64,
        tx: u32,
        kind: u8,
        done: bool,
        progress: i32,
    ) -> Vec<Cue> {
        let key = (session, tx, kind);
        if kind > 2 || progress < 0 {
            return Vec::new();
        }
        let mut cues = Vec::new();
        if self.current != Some(key) || (!done && self.finished && progress == 0) {
            self.current = Some(key);
            self.last_second = -1;
            self.finished = false;
        }
        if done {
            if !self.finished {
                self.finished = true;
                cues.push(Cue::Finished(kind));
            }
            return cues;
        }
        if self.finished {
            return cues;
        }
        let second = progress / 1000;
        if second <= self.last_second {
            return cues;
        }
        if self.last_second == -1 && kind != 0 {
            cues.push(Cue::Initial(kind));
        }
        if second >= 1 && kind != 0 {
            let step = second.rem_euclid(4) as u8;
            cues.push(Cue::Tick(kind, if step < 3 { step } else { 4 - step }));
        }
        self.last_second = second;
        cues
    }
    pub fn pause(&mut self, paused: bool) -> Option<Cue> {
        let old = self.pause.replace(paused);
        old.filter(|v| *v != paused).map(|_| Cue::Pause(paused))
    }
    pub fn tap(&mut self) -> Cue {
        let cue = Cue::Tap(self.tap);
        self.tap = (self.tap + 1) % 5;
        cue
    }
    pub fn disconnected(&mut self) {
        *self = Self::default();
    }
}
#[cfg(feature = "desktop")]
pub struct Player {
    sender: std::sync::mpsc::SyncSender<(Cue, f32)>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
#[cfg(feature = "desktop")]
impl Player {
    pub fn new() -> Self {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let (sender, receiver) = mpsc::sync_channel::<(Cue, f32)>(16);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = std::thread::spawn(move || {
            let output = rodio::OutputStream::try_default();
            let Ok((_stream, handle)) = output else {
                crate::logging::write(
                    crate::log_level::LogLevel::Warn,
                    "audio",
                    "No audio output device is available",
                );
                while !stopping.load(Ordering::Relaxed) {
                    let _ = receiver.recv_timeout(std::time::Duration::from_millis(100));
                }
                return;
            };
            let mut voices = Vec::<rodio::Sink>::new();
            while !stopping.load(Ordering::Relaxed) {
                let Ok((cue, volume)) =
                    receiver.recv_timeout(std::time::Duration::from_millis(100))
                else {
                    voices.retain(|s| !s.empty());
                    continue;
                };
                if !volume.is_finite() || volume <= 0.0 {
                    continue;
                }
                if matches!(cue, Cue::Initial(_)) {
                    for voice in voices.drain(..) {
                        voice.stop();
                    }
                }
                let Ok(sink) = rodio::Sink::try_new(&handle) else {
                    continue;
                };
                sink.set_volume((volume.powf(std::f32::consts::E) + 0.05).min(1.0));
                if let Cue::Tap(index) = cue {
                    let frequencies: [[f32; 3]; 5] = [
                        [164.81, 196.0, 246.94],
                        [196.0, 246.94, 293.66],
                        [246.94, 293.66, 369.99],
                        [293.66, 369.99, 440.0],
                        [369.99, 440.0, 554.37],
                    ];
                    let mut samples = Vec::with_capacity(44100);
                    for frame in 0..44100 {
                        let t = frame as f32 / 44100.0;
                        let value = frequencies[index as usize % 5]
                            .iter()
                            .map(|f| (t * f * std::f32::consts::TAU).sin() * (-t * 8.0).exp())
                            .sum::<f32>()
                            / 6.0;
                        samples.push(value);
                    }
                    sink.append(rodio::buffer::SamplesBuffer::new(1, 44100, samples));
                } else if let Some(bytes) = asset(cue)
                    && let Ok(source) = rodio::Decoder::new(std::io::Cursor::new(bytes))
                {
                    sink.append(source);
                }
                voices.retain(|s| !s.empty());
                if voices.len() >= 8 {
                    voices.remove(0).stop();
                }
                voices.push(sink);
                if matches!(cue, Cue::Finished(1 | 2))
                    && let Ok(mew) = rodio::Sink::try_new(&handle)
                {
                    mew.set_volume(volume.min(1.0));
                    if let Ok(source) = rodio::Decoder::new(std::io::Cursor::new(
                        include_bytes!("../../gui/public/sounds/mew.ogg").as_slice(),
                    )) {
                        mew.append(source);
                        voices.push(mew);
                    }
                }
            }
        });
        Self {
            sender,
            stop,
            worker: Some(worker),
        }
    }
    pub fn play(&self, cue: Cue, volume: f32) {
        let _ = self.sender.try_send((cue, volume));
    }
}
#[cfg(feature = "desktop")]
impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(feature = "desktop")]
impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(feature = "desktop")]
fn asset(cue: Cue) -> Option<&'static [u8]> {
    macro_rules! sound {
        ($path:literal) => {
            Some(include_bytes!(concat!("../../gui/public/sounds/", $path)).as_slice())
        };
    }
    match cue {
        Cue::Initial(1) => sound!("full-reset/init-full-reset-with-tail.ogg"),
        Cue::Initial(2) => sound!("mounting-reset/init-mounting-reset-with-tail.ogg"),
        Cue::Tick(1, 0) => sound!("full-reset/full-click-1.ogg"),
        Cue::Tick(1, 1) => sound!("full-reset/full-click-2.ogg"),
        Cue::Tick(1, _) => sound!("full-reset/full-click-3.ogg"),
        Cue::Tick(2, 0) => sound!("mounting-reset/mount-click-1.ogg"),
        Cue::Tick(2, 1) => sound!("mounting-reset/mount-click-2.ogg"),
        Cue::Tick(2, _) => sound!("mounting-reset/mount-click-3.ogg"),
        Cue::Finished(0) => sound!("yaw-reset/yaw-reset.ogg"),
        Cue::Finished(1) => sound!("full-reset/end-full-reset-with-tail.ogg"),
        Cue::Finished(2) => sound!("mounting-reset/end-mounting-reset-with-tail.ogg"),
        Cue::Pause(true) => sound!("tracking/pause.ogg"),
        Cue::Pause(false) => sound!("tracking/play.ogg"),
        _ => None,
    }
}
