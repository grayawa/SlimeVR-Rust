//! Bound UDP work between timer polls; a datagram/bundle is always atomic.
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::time::Interval;

pub(super) enum Turn<T> {
    Tick,
    Activity(T),
}
pub(super) async fn schedule<T>(
    clock: &mut Interval,
    activity: impl Future<Output = T>,
    prioritize_tick: bool,
) -> Turn<T> {
    tokio::pin!(activity);
    if prioritize_tick {
        tokio::select! {biased;
            _ = clock.tick() => Turn::Tick,
            value = &mut activity => Turn::Activity(value),
        }
    } else {
        // Give ready input a turn after a tick that exceeded its period.
        tokio::select! {biased;
            value = &mut activity => Turn::Activity(value),
            _ = clock.tick() => Turn::Tick,
        }
    }
}

pub(super) struct Budget {
    start: Instant,
    limit: Duration,
}
impl Budget {
    pub fn new(start: Instant, period: Duration) -> Self {
        Self {
            start,
            limit: (period / 4).min(Duration::from_millis(1)),
        }
    }
    pub fn exhausted(&self, handled: usize, now: Instant) -> bool {
        handled >= 16 || now.saturating_duration_since(self.start) >= self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_count_and_time_each_return_control_to_timer_without_splitting_a_datagram() {
        let now = Instant::now();
        let budget = Budget::new(now, Duration::from_millis(4));
        assert!(!budget.exhausted(15, now + Duration::from_micros(999)));
        assert!(budget.exhausted(16, now));
        assert!(budget.exhausted(1, now + Duration::from_millis(1)));
        assert!(Budget::new(now, Duration::from_millis(1))
            .exhausted(1, now + Duration::from_micros(250)));
        assert!(Budget::new(now, Duration::from_millis(50))
            .exhausted(1, now + Duration::from_millis(1)));
    }
    #[tokio::test(start_paused = true)]
    async fn due_ticks_and_ready_inputs_cannot_starve_each_other_under_sustained_overload() {
        let mut clock = tokio::time::interval(Duration::from_millis(4));
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        clock.tick().await;
        tokio::time::advance(Duration::from_millis(4)).await;
        assert!(matches!(
            schedule(&mut clock, std::future::ready(42), true).await,
            Turn::Tick
        ));
        tokio::time::advance(Duration::from_millis(4)).await;
        assert!(matches!(
            schedule(&mut clock, std::future::ready(42), false).await,
            Turn::Activity(42)
        ));
        assert!(matches!(
            schedule(&mut clock, std::future::ready(42), true).await,
            Turn::Tick
        ));
        // With no inputs, opting to poll input first must still allow timer progress.
        assert!(matches!(
            schedule(&mut clock, std::future::pending::<()>(), false).await,
            Turn::Tick
        ));
    }
}
