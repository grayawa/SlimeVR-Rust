//! Original SlimeVR battery display rules, independent of the native renderer.
use crate::protocol::Tracker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Background,
    Success,
    Warning,
    Critical,
    Disabled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    None,
    Charging,
    Charged,
}
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    pub raw_percent: u8,
    pub percent: u8,
    pub voltage: Option<f32>,
    pub runtime_us: Option<u64>,
    pub disabled: bool,
}
impl Reading {
    pub fn from_tracker(tracker: &Tracker) -> Option<Self> {
        let raw_percent = tracker.battery?;
        Some(Self {
            raw_percent,
            // Original UByte sentinels (e.g. -1 cast to 255) mean empty,
            // while 101..=200 represent a fully charged battery.
            percent: if raw_percent > 200 {
                0
            } else {
                raw_percent.min(100)
            },
            voltage: tracker.voltage.filter(|v| v.is_finite() && *v > 0.),
            runtime_us: tracker.battery_runtime_us.filter(|v| *v > 0),
            disabled: tracker.status == 1,
        })
    }
    pub fn charging(self) -> bool {
        self.voltage.is_some_and(|v| v > 4.3)
    }
    pub fn marker(self) -> Marker {
        if !self.charging() {
            Marker::None
        } else if (101..=200).contains(&self.raw_percent) {
            Marker::Charged
        } else {
            Marker::Charging
        }
    }
    pub fn fill(self) -> f32 {
        if self.charging() {
            1.
        } else {
            f32::from(self.percent) / 100.
        }
    }
    pub fn color(self) -> Color {
        if self.disabled {
            Color::Disabled
        } else if self.charging() || self.percent > 40 {
            Color::Success
        } else if self.percent > 20 {
            Color::Warning
        } else if self.percent > 0 {
            Color::Critical
        } else {
            Color::Background
        }
    }
    pub fn runtime_text(self) -> Option<String> {
        self.runtime_us.map(|us| {
            format!(
                "{}h {}min",
                us / 3_600_000_000,
                us % 3_600_000_000 / 60_000_000
            )
        })
    }
    pub fn lines(self, debug: bool) -> Vec<String> {
        if self.charging() {
            return Vec::new();
        }
        let mut lines = Vec::new();
        if let Some(runtime) = self.runtime_text() {
            lines.push(runtime);
        }
        if self.runtime_us.is_none() || debug {
            lines.push(format!("{}%", self.percent));
        }
        lines
    }
}
