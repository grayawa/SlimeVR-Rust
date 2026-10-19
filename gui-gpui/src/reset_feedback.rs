//! Backend-driven reset feedback using the original toolbar's timing and easing.
use crate::protocol::ResetProgress;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub counting: bool,
    pub progress: f32,
    pub remaining_ms: i32,
    pub timer_scale: f32,
    pub completion: f32,
    pub outline: f32,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            counting: false,
            progress: 0.,
            remaining_ms: 0,
            timer_scale: 1.,
            completion: 1.,
            outline: 0.,
        }
    }
}
impl Frame {
    pub fn timer(self) -> String {
        let tenths = self.remaining_ms.max(0).saturating_add(50) / 100;
        if tenths % 10 == 0 {
            format!("{}s", tenths / 10)
        } else {
            format!("{}.{:01}s", tenths / 10, tenths % 10)
        }
    }
    /// Original one-second skiing keyframes: angle in degrees, translation in icon fractions.
    pub fn skiing(self) -> (f32, f32, f32) {
        const KEYS: [(f32, f32, f32); 11] = [
            (0., 0., 0.),
            (12., -0.05, 0.05),
            (10., 0., 0.),
            (12., 0.05, -0.05),
            (10., 0., 0.),
            (12., -0.05, 0.05),
            (10., 0., 0.),
            (12., 0.05, -0.05),
            (10., 0., 0.),
            (10., -0.05, 0.05),
            (0., 0., 0.),
        ];
        let t = self.completion.clamp(0., 1.) * 10.;
        let index = (t as usize).min(9);
        let f = t - index as f32;
        let a = KEYS[index];
        let b = KEYS[index + 1];
        (
            a.0 + (b.0 - a.0) * f,
            a.1 + (b.1 - a.1) * f,
            a.2 + (b.2 - a.2) * f,
        )
    }
}

struct Timeline {
    packet: ResetProgress,
    changed: Instant,
    started: Instant,
    finished: Option<Instant>,
    from: f32,
    target: f32,
}
impl Timeline {
    fn width(&self, now: Instant) -> f32 {
        let seconds = now.saturating_duration_since(self.changed).as_secs_f32();
        let f = if self.packet.done {
            seconds.min(1.)
        } else {
            spring((seconds / 0.3).min(1.))
        };
        self.from + (self.target - self.from) * f
    }
    fn frame(&self, now: Instant) -> Frame {
        let elapsed = self
            .finished
            .map(|at| now.saturating_duration_since(at).as_secs_f32());
        let phase = now.saturating_duration_since(self.started).as_secs_f32() % 1.;
        let timer_scale = if phase < 0.2 {
            1. + phase * 1.5
        } else if phase < 0.4 {
            1.3 - (phase - 0.2) * 1.5
        } else {
            1.
        };
        Frame {
            counting: !self.packet.done,
            progress: self.width(now).clamp(0., 1.),
            remaining_ms: self
                .packet
                .duration_ms
                .saturating_sub(self.packet.progress_ms)
                .max(0),
            timer_scale,
            completion: elapsed.unwrap_or(1.).min(1.),
            outline: elapsed.map_or(0., |t| {
                if t <= 2. {
                    1.
                } else {
                    1. - bezier(((t - 2.) / 0.5).clamp(0., 1.), (0.42, 0., 0.58, 1.))
                }
            }),
        }
    }
    fn active(&self, now: Instant) -> bool {
        self.finished
            .is_none_or(|at| now.saturating_duration_since(at) < Duration::from_millis(2500))
    }
}

#[derive(Default)]
pub struct Feedback {
    session: u64,
    last: Option<ResetProgress>,
    tracks: BTreeMap<(u8, Vec<u8>), Timeline>,
}
impl Feedback {
    pub fn observe(&mut self, session: u64, packet: Option<&ResetProgress>, now: Instant) {
        if self.session != session {
            *self = Self {
                session,
                ..Default::default()
            };
        }
        let Some(packet) = packet else {
            return;
        };
        if self.last.as_ref() == Some(packet) {
            return;
        }
        self.last = Some(packet.clone());
        let mut parts = packet.body_parts.clone();
        parts.sort_unstable();
        parts.dedup();
        self.tracks.retain(|key, track| {
            track.active(now) && (track.packet.done || *key == (packet.kind, parts.clone()))
        });
        // Keep the matching countdown's current width across progress notifications.
        self.apply(packet, now);
    }
    fn apply(&mut self, packet: &ResetProgress, now: Instant) {
        let mut parts = packet.body_parts.clone();
        parts.sort_unstable();
        parts.dedup();
        let key = (packet.kind, parts);
        let previous = self.tracks.remove(&key);
        let continuing = previous
            .as_ref()
            .is_some_and(|t| t.packet.tx == packet.tx && !t.packet.done);
        let from = previous.as_ref().map_or(0., |t| t.width(now));
        let finished = if packet.done {
            Some(
                previous
                    .as_ref()
                    .filter(|t| t.packet.tx == packet.tx)
                    .and_then(|t| t.finished)
                    .unwrap_or(now),
            )
        } else {
            None
        };
        self.tracks.insert(
            key,
            Timeline {
                packet: packet.clone(),
                changed: now,
                started: if continuing {
                    previous.as_ref().unwrap().started
                } else {
                    now
                },
                finished,
                from,
                target: if packet.done || packet.duration_ms <= 0 {
                    0.
                } else {
                    (packet.progress_ms as f32 / packet.duration_ms as f32).clamp(0., 1.)
                },
            },
        );
    }
    pub fn frame(&self, kind: u8, parts: &[u8], now: Instant) -> Frame {
        let mut parts = parts.to_vec();
        parts.sort_unstable();
        parts.dedup();
        self.tracks
            .get(&(kind, parts))
            .filter(|t| t.active(now))
            .map_or_else(Frame::default, |t| t.frame(now))
    }
    pub fn animating(&self, now: Instant) -> bool {
        self.tracks.values().any(|t| t.active(now))
    }
    /// Advance visible feedback and request one final repaint when its outline expires.
    pub fn advance(&mut self, now: Instant) -> bool {
        let repaint = !self.tracks.is_empty();
        self.tracks.retain(|_, t| t.active(now));
        repaint
    }
    pub fn cancel_countdown(&mut self) {
        self.tracks.retain(|_, t| t.packet.done);
    }
}

/// CSS cubic-bezier(0.68, -0.8, 0.32, 1.8), including its overshoot.
fn spring(x: f32) -> f32 {
    bezier(x, (0.68, -0.8, 0.32, 1.8))
}
fn bezier(x: f32, (x1, y1, x2, y2): (f32, f32, f32, f32)) -> f32 {
    fn curve(t: f32, a: f32, b: f32) -> f32 {
        3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * b + t * t * t
    }
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..20 {
        let mid = (lo + hi) * 0.5;
        if curve(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    curve((lo + hi) * 0.5, y1, y2)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(kind: u8, parts: &[u8], progress_ms: i32, done: bool) -> ResetProgress {
        ResetProgress {
            tx: 7,
            kind,
            done,
            progress_ms,
            duration_ms: 3000,
            body_parts: parts.to_vec(),
        }
    }
    #[test]
    fn server_progress_drives_timer_while_fill_springs_between_notifications() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        feedback.observe(1, Some(&packet(1, &[], 0, false)), now);
        let later = now + Duration::from_secs(1);
        feedback.observe(1, Some(&packet(1, &[], 1000, false)), later);
        assert_eq!(feedback.frame(1, &[], later).timer(), "2s");
        let early = feedback.frame(1, &[], later + Duration::from_millis(40));
        assert!(early.progress <= 0.01);
        let overshoot = feedback.frame(1, &[], later + Duration::from_millis(240));
        assert!(overshoot.progress > 1. / 3.);
        assert!(
            (feedback
                .frame(1, &[], later + Duration::from_millis(310))
                .progress
                - 1. / 3.)
                .abs()
                < 1e-5
        );
        assert_eq!(
            feedback
                .frame(1, &[], later + Duration::from_secs(20))
                .timer(),
            "2s"
        );
        assert!(
            feedback
                .frame(1, &[], later + Duration::from_secs(20))
                .counting
        );
    }
    #[test]
    fn completion_spins_once_holds_outline_then_fades_and_duplicate_finish_keeps_clock() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        let done = packet(1, &[], 3000, true);
        feedback.observe(1, Some(&done), now);
        feedback.observe(1, Some(&done), now + Duration::from_millis(600));
        let frame = feedback.frame(1, &[], now + Duration::from_millis(500));
        assert!(!frame.counting);
        assert!((frame.completion - 0.5).abs() < 1e-5);
        assert_eq!(
            feedback
                .frame(1, &[], now + Duration::from_secs(1))
                .completion,
            1.
        );
        assert_eq!(
            feedback.frame(1, &[], now + Duration::from_secs(2)).outline,
            1.
        );
        assert!(
            (feedback
                .frame(1, &[], now + Duration::from_millis(2250))
                .outline
                - 0.5)
                .abs()
                < 1e-5
        );
        assert!(feedback.advance(now + Duration::from_millis(2501)));
        assert!(!feedback.advance(now + Duration::from_millis(2517)));
        assert_eq!(
            feedback.frame(1, &[], now + Duration::from_secs(3)).outline,
            0.
        );
    }
    #[test]
    fn mounting_groups_and_sessions_have_separate_feedback_and_reused_hotkey_tx_restarts() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        let mut feet = packet(2, &[11, 12], 0, false);
        feet.tx = 0;
        feedback.observe(1, Some(&feet), now);
        assert!(feedback.frame(2, &[12, 11], now).counting);
        assert!(!feedback.frame(2, &[], now).counting);
        feet.done = true;
        feet.progress_ms = 3000;
        feedback.observe(1, Some(&feet), now + Duration::from_secs(3));
        feet.done = false;
        feet.progress_ms = 0;
        feedback.observe(1, Some(&feet), now + Duration::from_secs(4));
        assert!(
            feedback
                .frame(2, &[11, 12], now + Duration::from_secs(4))
                .counting
        );
        feedback.observe(2, None, now + Duration::from_secs(4));
        assert!(!feedback.animating(now + Duration::from_secs(4)));
    }
    #[test]
    fn cancel_and_another_reset_clear_countdown_and_keep_completed_feedback() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        feedback.observe(1, Some(&packet(1, &[], 3000, true)), now);
        feedback.observe(1, Some(&packet(2, &[], 0, false)), now);
        assert_eq!(feedback.frame(1, &[], now).outline, 1.);
        feedback.cancel_countdown();
        assert!(!feedback.frame(2, &[], now).counting);
        assert_eq!(feedback.frame(1, &[], now).outline, 1.);
    }
    #[test]
    fn skiing_keyframes_timer_pulse_and_fractional_seconds_follow_original_values() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        feedback.observe(1, Some(&packet(2, &[], 3000, true)), now);
        assert_eq!(
            feedback
                .frame(2, &[], now + Duration::from_millis(100))
                .skiing(),
            (12., -0.05, 0.05)
        );
        assert_eq!(
            feedback
                .frame(2, &[], now + Duration::from_secs(1))
                .skiing(),
            (0., 0., 0.)
        );
        let mut reset = packet(1, &[], 0, false);
        reset.duration_ms = 3500;
        feedback.observe(1, Some(&reset), now);
        assert_eq!(feedback.frame(1, &[], now).timer(), "3.5s");
        assert!(
            (feedback
                .frame(1, &[], now + Duration::from_millis(200))
                .timer_scale
                - 1.3)
                .abs()
                < 1e-5
        );
    }
}
