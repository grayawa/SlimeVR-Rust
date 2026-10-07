use crate::Quaternion;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterType {
    None,
    Smoothing,
    #[default]
    Prediction,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FilterConfig {
    pub mode: FilterType,
    pub amount: f32,
}
impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            mode: FilterType::Prediction,
            amount: 0.2,
        }
    }
}
impl FilterConfig {
    pub fn validate(self) -> Result<(), String> {
        if !self.amount.is_finite() || self.amount < 0.0 || self.amount > 100.0 {
            Err("filter amount must be finite and in 0..100".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
pub struct QuaternionFilter {
    config: FilterConfig,
    latest: Quaternion,
    smoothing_start: Quaternion,
    filtered: Quaternion,
    deltas: VecDeque<Quaternion>,
    since_update: f32,
    pub impact: f32,
}
impl QuaternionFilter {
    pub fn new(config: FilterConfig, initial: Quaternion) -> Self {
        let mut filter = Self {
            config,
            latest: initial,
            smoothing_start: initial,
            filtered: initial,
            deltas: VecDeque::with_capacity(6),
            since_update: 0.0,
            impact: 0.0,
        };
        filter.reset(initial, initial);
        filter
    }
    pub fn output(&self) -> Quaternion {
        self.filtered
    }
    pub fn add(&mut self, q: Quaternion) {
        let old = self.latest;
        let new = q.twin_nearest(old);
        self.latest = new;
        match self.config.mode {
            FilterType::None => self.filtered = new,
            FilterType::Smoothing => {
                self.since_update = 0.0;
                self.smoothing_start = self.filtered;
            }
            FilterType::Prediction => {
                if self.deltas.len() == 6 {
                    self.deltas.pop_front();
                }
                self.deltas.push_back(old.inv() * new);
            }
        }
    }
    pub fn reset(&mut self, q: Quaternion, reference: Quaternion) {
        let q = q.twin_nearest(reference);
        self.deltas.clear();
        self.latest = q;
        self.filtered = q;
        self.add(q);
    }
    pub fn update(&mut self, dt: f32) {
        match self.config.mode {
            FilterType::None => {}
            FilterType::Smoothing => {
                self.since_update += dt;
                let mut factor = 42.0 * (1.0 - self.config.amount.min(1.0)) + 11.0;
                if self.config.amount > 1.0 {
                    factor /= self.config.amount;
                }
                self.filtered = self
                    .smoothing_start
                    .interp_q(self.latest, (factor * self.since_update).min(1.0));
            }
            FilterType::Prediction => {
                if !self.deltas.is_empty() {
                    let target = self.deltas.iter().fold(self.latest, |q, delta| q * *delta);
                    self.filtered = self
                        .filtered
                        .interp_q(target, ((15.0 * self.config.amount + 10.0) * dt).min(1.0));
                }
            }
        }
        self.impact = self.latest.angle_to_r(self.filtered);
    }
}
