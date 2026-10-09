//! Bounded timing telemetry. One histogram update per tick; no samples to sort.
use serde::Serialize;
use std::time::{Duration, Instant};

// Microsecond buckets: exact below 512 us, then <= 1/256 relative bucket width.
// Cover the full u64 microsecond range, preserving long-stall quantiles.
const LINEAR: usize = 512;
const SUB_BUCKETS: usize = 256;
const BUCKETS: usize = LINEAR + (64 - 9) * SUB_BUCKETS;

struct Histogram {
    buckets: Box<[u64]>,
    count: u64,
    max: Duration,
}
impl Histogram {
    fn new() -> Self {
        Self {
            buckets: vec![0; BUCKETS].into_boxed_slice(),
            count: 0,
            max: Duration::ZERO,
        }
    }
    fn index(us: u64) -> usize {
        if us < LINEAR as u64 {
            return us as usize;
        }
        let shift = (63 - us.leading_zeros()) as usize - 8;
        LINEAR + (shift - 1) * SUB_BUCKETS + (us >> shift) as usize - SUB_BUCKETS
    }
    fn upper_us(index: usize) -> u64 {
        if index < LINEAR {
            return index as u64;
        }
        let shift = (index - LINEAR) / SUB_BUCKETS + 1;
        let mantissa = (index - LINEAR) % SUB_BUCKETS + SUB_BUCKETS;
        ((((mantissa + 1) as u128) << shift) - 1).min(u64::MAX as u128) as u64
    }
    fn record(&mut self, duration: Duration) {
        let us = duration.as_nanos().div_ceil(1000).min(u64::MAX as u128) as u64;
        self.buckets[Self::index(us)] += 1;
        self.count += 1;
        self.max = self.max.max(duration);
    }
    fn percentiles(&self) -> Option<Percentiles> {
        if self.count == 0 {
            return None;
        }
        let ranks = [500u128, 950, 990, 999].map(|p| (self.count as u128 * p).div_ceil(1000));
        let max = self.max.as_secs_f64() * 1000.;
        let mut values = [0.; 4];
        let mut found = 0;
        let mut cumulative = 0u128;
        for (index, count) in self.buckets.iter().enumerate() {
            cumulative += *count as u128;
            while found < ranks.len() && cumulative >= ranks[found] {
                values[found] = (Self::upper_us(index) as f64 / 1000.).min(max);
                found += 1;
            }
            if found == ranks.len() {
                break;
            }
        }
        Some(Percentiles {
            p50: values[0],
            p95: values[1],
            p99: values[2],
            p999: values[3],
            max,
        })
    }
    fn clear(&mut self) {
        self.buckets.fill(0);
        self.count = 0;
        self.max = Duration::ZERO;
    }
}

#[derive(Debug, Serialize)]
pub(super) struct Percentiles {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
    pub max: f64,
}
#[derive(Debug, Default, Serialize)]
pub(super) struct Stalls {
    pub gt_2ms: u64,
    pub gt_5ms: u64,
    pub gt_10ms: u64,
    pub gt_50ms: u64,
    pub gt_100ms: u64,
}
impl Stalls {
    fn record(&mut self, excess: Duration) {
        // Cumulative, strictly greater-than: a 101 ms excess increments all five.
        for (threshold, count) in [
            (2, &mut self.gt_2ms),
            (5, &mut self.gt_5ms),
            (10, &mut self.gt_10ms),
            (50, &mut self.gt_50ms),
            (100, &mut self.gt_100ms),
        ] {
            if excess > Duration::from_millis(threshold) {
                *count += 1;
            }
        }
    }
}
#[derive(Debug, Serialize)]
pub(super) struct Report {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub tick_kind: &'static str,
    pub at_ms: u64,
    pub window_ms: f64,
    pub expected_tick_ms: f64,
    pub ticks: u64,
    pub interval_samples: u64,
    pub tick_jitter_ms: Option<Percentiles>,
    pub tick_work_ms: Option<Percentiles>,
    pub runtime_stall: Stalls,
    pub udp_datagrams: u64,
    pub udp_batches: u64,
    pub udp_budget_yields: u64,
    pub udp_coalesced: u64,
    pub udp_dropped_poses: u64,
    pub udp_dropped_controls: u64,
    pub udp_queue_delay_ms: Option<Percentiles>,
    pub udp_batch_work_ms: Option<Percentiles>,
    pub api_live_snapshots: u64,
    pub api_live_snapshot_ms: Option<Percentiles>,
    #[serde(flatten)]
    pub steamvr_output: crate::steamvr::OutputCounters,
}

pub(super) struct Timing {
    previous: Option<Instant>,
    window_start: Instant,
    window: Duration,
    period: Duration,
    pose: bool,
    jitter: Histogram,
    work: Histogram,
    stalls: Stalls,
    udp_delay: Histogram,
    udp_work: Histogram,
    live_snapshot: Histogram,
    udp_yields: u64,
    udp_loss: [u64; 3],
}
impl Timing {
    pub fn new(start: Instant, period: Duration, window: Duration, pose: bool) -> Self {
        Self {
            previous: None,
            window_start: start,
            window,
            period,
            pose,
            jitter: Histogram::new(),
            work: Histogram::new(),
            stalls: Stalls::default(),
            udp_delay: Histogram::new(),
            udp_work: Histogram::new(),
            live_snapshot: Histogram::new(),
            udp_yields: 0,
            udp_loss: [0; 3],
        }
    }
    pub fn begin_tick(&mut self, now: Instant) {
        if let Some(previous) = self.previous {
            let gap = now.saturating_duration_since(previous);
            self.jitter.record(gap.abs_diff(self.period));
            self.stalls.record(gap.saturating_sub(self.period));
        }
        self.previous = Some(now);
    }
    pub fn end_tick(&mut self, elapsed: Duration) {
        self.work.record(elapsed);
    }
    pub fn udp_received(&mut self, queue_delay: Duration) {
        self.udp_delay.record(queue_delay);
    }
    pub fn udp_batch(&mut self, elapsed: Duration, yielded: bool) {
        self.udp_work.record(elapsed);
        self.udp_yields += u64::from(yielded);
    }
    pub fn live_snapshot(&mut self, elapsed: Duration) {
        self.live_snapshot.record(elapsed);
    }
    pub fn ingress_counts(&mut self, coalesced: u64, dropped_poses: u64, dropped_controls: u64) {
        for (total, count) in
            self.udp_loss
                .iter_mut()
                .zip([coalesced, dropped_poses, dropped_controls])
        {
            *total += count;
        }
    }
    pub fn report(&mut self, now: Instant, at_ms: u64, force: bool) -> Option<Report> {
        if self.work.count == 0 || (!force && now.duration_since(self.window_start) < self.window) {
            return None;
        }
        let report = Report {
            kind: "runtime_timing",
            tick_kind: if self.pose { "pose" } else { "receiver" },
            at_ms,
            window_ms: now.duration_since(self.window_start).as_secs_f64() * 1000.,
            expected_tick_ms: self.period.as_secs_f64() * 1000.,
            ticks: self.work.count,
            interval_samples: self.jitter.count,
            tick_jitter_ms: self.jitter.percentiles(),
            tick_work_ms: self.work.percentiles(),
            runtime_stall: std::mem::take(&mut self.stalls),
            udp_datagrams: self.udp_delay.count,
            udp_batches: self.udp_work.count,
            udp_budget_yields: std::mem::take(&mut self.udp_yields),
            udp_coalesced: self.udp_loss[0],
            udp_dropped_poses: self.udp_loss[1],
            udp_dropped_controls: self.udp_loss[2],
            udp_queue_delay_ms: self.udp_delay.percentiles(),
            udp_batch_work_ms: self.udp_work.percentiles(),
            api_live_snapshots: self.live_snapshot.count,
            api_live_snapshot_ms: self.live_snapshot.percentiles(),
            steamvr_output: Default::default(),
        };
        self.jitter.clear();
        self.work.clear();
        self.udp_delay.clear();
        self.udp_work.clear();
        self.live_snapshot.clear();
        self.udp_loss = [0; 3];
        self.window_start = now;
        // Preserve the previous tick start to measure intervals across windows.
        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn udp_wait_work_and_capacity_counts_report_separately_without_changing_tick_jitter() {
        let start = Instant::now();
        let mut timing = Timing::new(
            start,
            Duration::from_millis(4),
            Duration::from_secs(1),
            true,
        );
        timing.begin_tick(start);
        timing.end_tick(Duration::from_micros(100));
        timing.udp_received(Duration::from_millis(100));
        timing.udp_batch(Duration::from_millis(2), true);
        timing.live_snapshot(Duration::from_millis(3));
        timing.ingress_counts(12, 1, 2);
        let report = timing
            .report(start + Duration::from_secs(1), 1000, false)
            .unwrap();
        assert_eq!(report.udp_datagrams, 1);
        assert_eq!(report.udp_batches, 1);
        assert_eq!(report.udp_budget_yields, 1);
        assert_eq!(report.udp_queue_delay_ms.unwrap().max, 100.);
        assert_eq!(report.udp_batch_work_ms.unwrap().max, 2.);
        assert_eq!(report.api_live_snapshots, 1);
        assert_eq!(report.api_live_snapshot_ms.unwrap().max, 3.);
        assert_eq!(
            [
                report.udp_coalesced,
                report.udp_dropped_poses,
                report.udp_dropped_controls
            ],
            [12, 1, 2]
        );
        assert!(report.tick_jitter_ms.is_none());
        timing.begin_tick(start + Duration::from_millis(1004));
        timing.end_tick(Duration::from_micros(100));
        let report = timing
            .report(start + Duration::from_secs(2), 2000, false)
            .unwrap();
        assert_eq!(report.udp_datagrams, 0);
        assert_eq!(report.udp_budget_yields, 0);
        assert!(report.udp_queue_delay_ms.is_none());
        assert_eq!(report.udp_coalesced, 0);
        assert_eq!(report.api_live_snapshots, 0);
        assert!(report.api_live_snapshot_ms.is_none());
    }
    #[test]
    fn fractional_excess_keeps_clock_precision_at_thresholds() {
        let mut stalls = Stalls::default();
        stalls.record(Duration::from_millis(2));
        assert_eq!(stalls.gt_2ms, 0);
        stalls.record(Duration::from_millis(2) + Duration::from_nanos(1));
        assert_eq!(stalls.gt_2ms, 1);
        assert_eq!(stalls.gt_5ms, 0);
        let mut histogram = Histogram::new();
        histogram.record(Duration::from_nanos(123));
        let p = histogram.percentiles().unwrap();
        assert_eq!(p.max, Duration::from_nanos(123).as_secs_f64() * 1000.);
        assert_eq!(p.p999, p.max);
    }
    #[test]
    fn quantile_ranks_and_extreme_values_fit_bounded_histogram() {
        let mut histogram = Histogram::new();
        assert!(histogram.percentiles().is_none());
        for us in 1..=1000 {
            histogram.record(Duration::from_micros(us));
        }
        let p = histogram.percentiles().unwrap();
        for (actual, expected) in [(p.p50, 0.5), (p.p95, 0.95), (p.p99, 0.99), (p.p999, 0.999)] {
            assert!(actual >= expected && actual - expected <= 0.004);
        }
        assert_eq!(p.max, 1.);
        for us in [
            0,
            1,
            511,
            512,
            1023,
            1024,
            10_000,
            100_000,
            1_000_000,
            u64::MAX,
        ] {
            let index = Histogram::index(us);
            assert!(index < BUCKETS);
            assert!(Histogram::upper_us(index) >= us);
            if us > 0 {
                assert!(
                    Histogram::upper_us(index) as u128 - us as u128 <= (us as u128 / 256).max(1)
                );
            }
        }
        histogram.clear();
        assert!(histogram.percentiles().is_none());
    }
    #[test]
    fn startup_boundaries_and_thresholds_use_excess_not_normal_period() {
        let start = Instant::now();
        let mut timing = Timing::new(
            start,
            Duration::from_millis(4),
            Duration::from_millis(10),
            true,
        );
        let tick = |timing: &mut Timing, ms| {
            timing.begin_tick(start + Duration::from_millis(ms));
            timing.end_tick(Duration::from_micros(100));
        };
        tick(&mut timing, 0);
        tick(&mut timing, 4);
        tick(&mut timing, 10); // excess exactly 2ms
        let first = timing
            .report(start + Duration::from_millis(10), 10, false)
            .unwrap();
        assert_eq!(first.ticks, 3);
        assert_eq!(first.interval_samples, 2);
        assert_eq!(first.runtime_stall.gt_2ms, 0);
        assert_eq!(first.tick_jitter_ms.unwrap().max, 2.);
        assert!(timing
            .report(start + Duration::from_millis(11), 11, false)
            .is_none());
        tick(&mut timing, 119); // gap 109ms => excess 105ms, crosses report boundary
        let second = timing
            .report(start + Duration::from_millis(119), 119, false)
            .unwrap();
        assert_eq!(second.interval_samples, 1);
        assert_eq!(second.tick_jitter_ms.unwrap().max, 105.);
        let stalls = second.runtime_stall;
        assert_eq!(
            [
                stalls.gt_2ms,
                stalls.gt_5ms,
                stalls.gt_10ms,
                stalls.gt_50ms,
                stalls.gt_100ms
            ],
            [1; 5]
        );
        tick(&mut timing, 120); // A short catch-up tick contributes jitter with zero stall.
        let last = timing
            .report(start + Duration::from_millis(120), 120, true)
            .unwrap();
        assert_eq!(last.tick_jitter_ms.unwrap().max, 3.);
        assert_eq!(last.runtime_stall.gt_2ms, 0);
        assert!(timing
            .report(start + Duration::from_millis(121), 121, true)
            .is_none());
    }
    #[test]
    fn percentile_tail_does_not_hide_one_long_stall_or_invent_missed_samples() {
        let mut histogram = Histogram::new();
        for _ in 0..999 {
            histogram.record(Duration::from_micros(100));
        }
        histogram.record(Duration::from_secs(2));
        let p = histogram.percentiles().unwrap();
        assert_eq!(p.p999, 0.1); // nearest-rank 999th of 1000
        assert_eq!(p.max, 2000.);
        let mut stalls = Stalls::default();
        for ms in [2, 5, 10, 50, 100, 101] {
            stalls.record(Duration::from_millis(ms));
        }
        assert_eq!(
            [
                stalls.gt_2ms,
                stalls.gt_5ms,
                stalls.gt_10ms,
                stalls.gt_50ms,
                stalls.gt_100ms
            ],
            [5, 4, 3, 2, 1]
        );
    }
}
