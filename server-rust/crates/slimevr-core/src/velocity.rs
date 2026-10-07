//! Linear velocity from successive positions, with the upstream 100 µs..250 ms window.
use crate::Vector3;

#[derive(Default)]
pub struct DerivedVelocity {
    previous: Option<(u64, Vector3)>,
}

impl DerivedVelocity {
    pub fn reset(&mut self) {
        self.previous = None;
    }

    /// Explicit monotonic microseconds; `None` invalidates the position history.
    /// Invalid intervals seed a new baseline, as Tracker.updateDerivedVelocity does.
    pub fn update(&mut self, at_us: u64, position: Option<Vector3>) -> Option<Vector3> {
        let Some(position) = position.filter(|p| p.is_finite()) else {
            self.reset();
            return None;
        };
        let previous = self.previous.replace((at_us, position));
        let (previous_at, previous_position) = previous?;
        let dt = at_us.checked_sub(previous_at)?;
        if !(100..=250_000).contains(&dt) {
            return None;
        }
        // Upstream divides using a double duration, then converts each component to f32.
        let seconds = dt as f64 / 1_000_000.0;
        let velocity = Vector3::new(
            ((position.x - previous_position.x) as f64 / seconds) as f32,
            ((position.y - previous_position.y) as f64 / seconds) as f32,
            ((position.z - previous_position.z) as f64 / seconds) as f32,
        );
        velocity.is_finite().then_some(velocity)
    }
}
