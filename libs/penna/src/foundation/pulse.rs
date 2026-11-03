use std::collections::HashMap;

use ala::{App, EntityId, Global};
use web_time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(33);

const LEASE: Duration = Duration::from_millis(300);

struct PulseClock {
    epoch: Instant,
    leases: HashMap<EntityId, Instant>,
    running: bool,
}

impl Global for PulseClock {}

impl Default for PulseClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            leases: HashMap::new(),
            running: false,
        }
    }
}

pub fn pulse_phase(period: Duration, view: EntityId, cx: &mut App) -> f32 {
    if cx.reduce_motion() {
        return 0.0;
    }

    let phase = phase_at(
        ala::animation_time(cx.default_global::<PulseClock>().epoch.elapsed()),
        period,
    );
    pulse_lease(view, cx);
    phase
}

pub(crate) fn pulse_lease(view: EntityId, cx: &mut App) {
    if cx.reduce_motion() {
        return;
    }

    let clock = cx.default_global::<PulseClock>();
    clock.leases.insert(view, Instant::now() + LEASE);
    let start = !clock.running;
    clock.running = true;

    if start {
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if cx.update(tick) {
                    break;
                }
            }
        })
        .detach();
    }
}

pub fn phase_at(elapsed: Duration, period: Duration) -> f32 {
    let period = period.as_nanos();
    if period == 0 {
        return 0.0;
    }
    let phase = (elapsed.as_nanos() % period) as f32 / period as f32;
    phase.min(1.0f32.next_down())
}

fn tick(cx: &mut App) -> bool {
    let clock = cx.default_global::<PulseClock>();
    if sweep(&mut clock.leases, Instant::now()) {
        clock.running = false;
        return true;
    }

    let views: Vec<EntityId> = clock.leases.keys().copied().collect();
    for view in views {
        cx.notify(view);
    }
    false
}

fn sweep(leases: &mut HashMap<EntityId, Instant>, now: Instant) -> bool {
    leases.retain(|_, until| *until > now);
    leases.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: Duration = Duration::from_millis(800);

    #[test]
    fn phase_starts_at_zero_and_climbs_within_the_period() {
        assert_eq!(phase_at(Duration::ZERO, PERIOD), 0.0);
        assert_eq!(phase_at(Duration::from_millis(200), PERIOD), 0.25);
        assert_eq!(phase_at(Duration::from_millis(400), PERIOD), 0.5);
    }

    #[test]
    fn phase_wraps_instead_of_reaching_one() {
        assert_eq!(phase_at(PERIOD, PERIOD), 0.0);
        assert_eq!(phase_at(Duration::from_millis(2200), PERIOD), 0.75);
        assert!(phase_at(Duration::from_nanos(799_999_999), PERIOD) < 1.0);
    }

    #[test]
    fn phase_is_drift_free_after_a_day() {
        let day = PERIOD * 108_000;
        assert_eq!(phase_at(day + Duration::from_millis(200), PERIOD), 0.25);
    }

    #[test]
    fn a_zero_period_freezes_rather_than_dividing_by_zero() {
        assert_eq!(phase_at(Duration::from_millis(200), Duration::ZERO), 0.0);
    }

    #[test]
    fn sweep_drops_lapsed_leases_and_parks_once_empty() {
        let now = Instant::now();
        let mut leases = HashMap::from([
            (EntityId::from(1), now + Duration::from_millis(100)),
            (EntityId::from(2), now + Duration::from_millis(300)),
        ]);

        assert!(!sweep(&mut leases, now + Duration::from_millis(200)));
        assert_eq!(leases.len(), 1);
        assert!(leases.contains_key(&EntityId::from(2)));

        assert!(sweep(&mut leases, now + Duration::from_millis(400)));
        assert!(leases.is_empty());
    }
}
