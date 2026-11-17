use std::collections::VecDeque;

use ala::{SpringConfig, SpringState, TouchPhase};
use web_time::{Duration, Instant};

const DISTANCE: f32 = 300.0;
const COMMIT: f32 = 1.0 / 3.0;
const FLING_VELOCITY: f32 = 500.0;
const OVERDRAG: f32 = 0.2;
const LIMIT: f32 = 1.2;
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);
const FRAME: Duration = Duration::from_millis(16);
const SETTLE_EPSILON: f32 = 0.001;
const SPRING: SpringConfig = SpringConfig::new(900.0, 60.0, 1.0);
const TAP_SPRING: SpringConfig = SpringConfig::new(320.0, 36.0, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiftEvent {
    Open,
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiftResponse {
    pub consumed: bool,
    pub started: bool,
    pub event: Option<LiftEvent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pull {
    pub side: f32,
    pub reach: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Motion {
    Idle,
    Drag {
        raw: f32,
    },
    Settle {
        from: f32,
        target: f32,
        state: SpringState,
        at: Instant,
        tapped: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Lift {
    progress: f32,
    motion: Motion,
    momentum: bool,
    armed: bool,
    pull: Option<Pull>,
    spring: Option<SpringConfig>,
    throw: Option<f32>,
    samples: VecDeque<(Instant, f32)>,
}

impl Default for Lift {
    fn default() -> Self {
        Self::new()
    }
}

impl Lift {
    pub fn new() -> Self {
        Self {
            progress: 0.0,
            motion: Motion::Idle,
            momentum: false,
            armed: false,
            pull: None,
            spring: None,
            throw: None,
            samples: VecDeque::new(),
        }
    }

    pub fn with_spring(spring: SpringConfig) -> Self {
        Self {
            spring: Some(spring),
            ..Self::new()
        }
    }

    pub fn scroll(&mut self, delta_y: f32, phase: TouchPhase, now: Instant) -> LiftResponse {
        let dragging = matches!(self.motion, Motion::Drag { .. });
        let mut response = LiftResponse::default();
        match phase {
            TouchPhase::Started => {
                self.momentum = false;
                let idle = self.motion == Motion::Idle && self.progress == 0.0;
                self.armed = idle && delta_y == 0.0;
                if idle && delta_y < 0.0 {
                    self.begin(delta_y, now, &mut response);
                }
            }
            TouchPhase::Moved if dragging => {
                self.drag(delta_y, now);
                response.consumed = true;
            }
            TouchPhase::Moved if self.armed && delta_y != 0.0 => {
                self.armed = false;
                if delta_y < 0.0 {
                    self.begin(delta_y, now, &mut response);
                }
            }
            TouchPhase::Moved => response.consumed = self.momentum,
            TouchPhase::Ended | TouchPhase::Cancelled if dragging => {
                self.drag(delta_y, now);
                let cancel = phase == TouchPhase::Cancelled;
                response.event = Some(self.release(cancel, now));
                self.momentum = !cancel;
                response.consumed = true;
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.armed = false;
                response.consumed = std::mem::take(&mut self.momentum);
            }
        }
        response
    }

    fn begin(&mut self, delta_y: f32, now: Instant, response: &mut LiftResponse) {
        self.samples.clear();
        self.pull = Some(Pull::default());
        self.throw = None;
        self.motion = Motion::Drag { raw: 0.0 };
        self.drag(delta_y, now);
        response.consumed = true;
        response.started = true;
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let Motion::Settle {
            from,
            target,
            state,
            at,
            tapped,
        } = self.motion
        else {
            return false;
        };
        let spring = self
            .spring
            .unwrap_or(if tapped { TAP_SPRING } else { SPRING });
        let elapsed = now.saturating_duration_since(at).as_secs_f32();
        let state = spring.step(state, 1.0, elapsed);
        if spring.is_settled(state, 1.0, SETTLE_EPSILON) {
            self.progress = target;
            self.motion = Motion::Idle;
            self.pull = None;
            return false;
        }
        self.progress = (from + (target - from) * state.position).clamp(0.0, LIMIT);
        self.motion = Motion::Settle {
            from,
            target,
            state,
            at: now,
            tapped,
        };
        true
    }

    pub fn animate(&mut self, open: bool, now: Instant) {
        let target = if open { 1.0 } else { 0.0 };
        let carried = match self.motion {
            Motion::Settle {
                from,
                target,
                state,
                ..
            } => state.velocity * (target - from),
            _ => 0.0,
        };
        let span = target - self.progress;
        self.samples.clear();
        self.pull = None;
        self.motion = if span == 0.0 {
            Motion::Idle
        } else {
            Motion::Settle {
                from: self.progress,
                target,
                state: SpringState {
                    position: 0.0,
                    velocity: carried / span,
                },
                at: now,
                tapped: true,
            }
        };
    }

    pub fn finish(&mut self) {
        if let Motion::Settle {
            from,
            target,
            at,
            tapped,
            ..
        } = self.motion
        {
            self.progress = target;
            self.motion = Motion::Settle {
                from,
                target,
                state: SpringState {
                    position: 1.0,
                    velocity: 0.0,
                },
                at,
                tapped,
            };
        }
    }

    pub fn snap(&mut self, open: bool) {
        self.progress = if open { 1.0 } else { 0.0 };
        self.motion = Motion::Idle;
        self.pull = None;
        self.samples.clear();
    }

    pub fn slide(&mut self, side: f32) {
        if let (Motion::Drag { .. }, Some(pull)) = (self.motion, &mut self.pull) {
            pull.side = side;
        }
    }

    pub fn pull(&self) -> Option<Pull> {
        self.pull
    }

    pub fn aim(&mut self, throw: f32) {
        if self.is_dragging() {
            self.throw = Some(throw);
        }
    }

    pub fn flight(&self) -> Option<f32> {
        match self.motion {
            Motion::Settle { state, .. } => Some(state.position),
            _ => None,
        }
    }

    pub fn will_open(&self) -> bool {
        self.is_dragging() && self.progress > COMMIT
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub fn is_dragging(&self) -> bool {
        matches!(self.motion, Motion::Drag { .. })
    }

    pub fn is_moving(&self) -> bool {
        self.motion != Motion::Idle
    }

    fn drag(&mut self, delta_y: f32, now: Instant) {
        let Motion::Drag { raw } = self.motion else {
            return;
        };
        let raw = raw - delta_y / DISTANCE;
        self.progress = if raw > 1.0 {
            (1.0 + (raw - 1.0) * OVERDRAG).min(LIMIT)
        } else {
            raw.max(0.0)
        };
        self.motion = Motion::Drag { raw };
        if let Some(pull) = &mut self.pull {
            pull.reach = raw.max(0.0) * DISTANCE;
        }
        self.samples.push_back((now, -delta_y));
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) > VELOCITY_WINDOW)
        {
            self.samples.pop_front();
        }
    }

    fn velocity(&self, now: Instant) -> f32 {
        let mut first = None;
        let mut total = 0.0;
        for (at, delta) in self
            .samples
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) <= VELOCITY_WINDOW)
        {
            first.get_or_insert(*at);
            total += delta;
        }
        let Some(first) = first else {
            return 0.0;
        };
        total
            / now
                .saturating_duration_since(first)
                .max(FRAME)
                .as_secs_f32()
    }

    fn release(&mut self, cancel: bool, now: Instant) -> LiftEvent {
        let velocity = self.velocity(now);
        let open = !cancel
            && velocity > -FLING_VELOCITY
            && (self.progress > COMMIT || velocity >= FLING_VELOCITY);
        let target = if open { 1.0 } else { 0.0 };
        let (frequency, _) = self.spring.unwrap_or(SPRING).canonical();
        let span = target - self.progress;
        let toward = match self.throw.filter(|throw| open && *throw > 0.0) {
            Some(throw) => velocity / throw,
            None if span != 0.0 => velocity / DISTANCE / span,
            None => 0.0,
        };
        self.samples.clear();
        self.motion = Motion::Settle {
            from: self.progress,
            target,
            state: SpringState {
                position: 0.0,
                velocity: toward.clamp(0.0, frequency),
            },
            at: now,
            tapped: false,
        };
        if open {
            LiftEvent::Open
        } else {
            LiftEvent::Close
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Clock(Instant);

    impl Clock {
        fn at(&self, ms: u64) -> Instant {
            self.0 + Duration::from_millis(ms)
        }
    }

    fn settle(lift: &mut Lift, clock: &Clock, from: u64) -> u64 {
        let mut ms = from;
        while lift.tick(clock.at(ms)) {
            ms += 16;
            assert!(ms < from + 2000, "lift never settled");
        }
        ms
    }

    fn slow_drag(lift: &mut Lift, clock: &Clock, deltas: &[f32]) -> u64 {
        let mut ms = 0;
        for (step, delta) in deltas.iter().enumerate() {
            let phase = if step == 0 {
                TouchPhase::Started
            } else {
                TouchPhase::Moved
            };
            assert!(lift.scroll(*delta, phase, clock.at(ms)).consumed);
            ms += 150;
        }
        ms + 200
    }

    #[test]
    fn the_lift_follows_the_finger_and_resists_past_the_top() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        let response = lift.scroll(-60.0, TouchPhase::Started, clock.at(0));
        assert!(response.started);
        assert!((lift.progress() - 0.2).abs() < 1e-4);
        lift.scroll(-300.0, TouchPhase::Moved, clock.at(16));
        assert!((lift.progress() - (1.0 + 0.2 * OVERDRAG)).abs() < 1e-4);
        lift.scroll(-3000.0, TouchPhase::Moved, clock.at(32));
        assert_eq!(lift.progress(), LIMIT);
        lift.scroll(6000.0, TouchPhase::Moved, clock.at(48));
        assert_eq!(lift.progress(), 0.0);
    }

    #[test]
    fn the_pull_keeps_the_finger_offset_until_the_lift_rests() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        assert_eq!(lift.pull(), None);
        lift.scroll(-90.0, TouchPhase::Started, clock.at(0));
        lift.slide(12.0);
        assert!(!lift.will_open());
        lift.scroll(-360.0, TouchPhase::Moved, clock.at(150));
        lift.slide(-40.0);
        let held = Pull {
            side: -40.0,
            reach: 450.0,
        };
        assert_eq!(lift.pull(), Some(held));
        assert!(lift.will_open());
        lift.scroll(0.0, TouchPhase::Ended, clock.at(400));
        lift.slide(99.0);
        assert_eq!(lift.pull(), Some(held));
        settle(&mut lift, &clock, 400);
        assert_eq!(lift.pull(), None);
    }

    #[test]
    fn a_release_flies_from_nought_to_one_at_the_fingers_speed() {
        let clock = Clock(Instant::now());
        let spring = SpringConfig::new(200.0, 28.5, 1.0);
        let flown = |lift: &Lift| lift.flight().expect("a released lift is in flight");

        let mut thrown = Lift::with_spring(spring);
        thrown.scroll(-80.0, TouchPhase::Started, clock.at(0));
        thrown.scroll(-80.0, TouchPhase::Moved, clock.at(16));
        thrown.scroll(-80.0, TouchPhase::Moved, clock.at(32));
        thrown.aim(240.0);
        thrown.scroll(0.0, TouchPhase::Ended, clock.at(48));
        assert_eq!(flown(&thrown), 0.0);
        thrown.tick(clock.at(148));
        assert!((flown(&thrown) - 0.757).abs() < 0.01, "{}", flown(&thrown));

        let mut rested = Lift::with_spring(spring);
        let release = slow_drag(&mut rested, &clock, &[-200.0, -200.0]);
        rested.aim(240.0);
        rested.scroll(0.0, TouchPhase::Ended, clock.at(release));
        assert_eq!(rested.progress(), 1.0 + (100.0 / DISTANCE) * OVERDRAG);
        rested.tick(clock.at(release + 100));
        assert!((flown(&rested) - 0.413).abs() < 0.01, "{}", flown(&rested));
        settle(&mut rested, &clock, release + 100);
        assert_eq!((rested.progress(), rested.flight()), (1.0, None));
    }

    #[test]
    fn past_a_third_opens_and_short_of_it_springs_back() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        let release = slow_drag(&mut lift, &clock, &[-60.0, -60.0]);
        let response = lift.scroll(0.0, TouchPhase::Ended, clock.at(release));
        assert_eq!(response.event, Some(LiftEvent::Open));
        settle(&mut lift, &clock, release);
        assert_eq!(lift.progress(), 1.0);
        assert!(!lift.is_moving());

        let mut lift = Lift::new();
        let release = slow_drag(&mut lift, &clock, &[-40.0, -40.0]);
        let response = lift.scroll(0.0, TouchPhase::Ended, clock.at(release));
        assert_eq!(response.event, Some(LiftEvent::Close));
        settle(&mut lift, &clock, release);
        assert_eq!(lift.progress(), 0.0);
    }

    #[test]
    fn a_flick_up_opens_and_a_flick_down_closes() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        lift.scroll(-12.0, TouchPhase::Started, clock.at(0));
        lift.scroll(-12.0, TouchPhase::Moved, clock.at(16));
        let response = lift.scroll(0.0, TouchPhase::Ended, clock.at(24));
        assert_eq!(response.event, Some(LiftEvent::Open));

        let mut lift = Lift::new();
        let ms = slow_drag(&mut lift, &clock, &[-80.0, -80.0, -80.0]);
        for step in 0..3 {
            lift.scroll(20.0, TouchPhase::Moved, clock.at(ms + step * 16));
        }
        let response = lift.scroll(0.0, TouchPhase::Ended, clock.at(ms + 48));
        assert_eq!(response.event, Some(LiftEvent::Close));
    }

    #[test]
    fn downward_and_busy_starts_are_left_alone() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        assert!(!lift.scroll(20.0, TouchPhase::Started, clock.at(0)).consumed);
        assert!(!lift.scroll(20.0, TouchPhase::Moved, clock.at(16)).consumed);
        lift.animate(true, clock.at(32));
        assert!(
            !lift
                .scroll(-20.0, TouchPhase::Started, clock.at(48))
                .consumed
        );
    }

    #[test]
    fn a_touch_that_catches_a_fling_lifts_on_its_first_upward_move() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        assert!(!lift.scroll(0.0, TouchPhase::Started, clock.at(0)).consumed);
        let response = lift.scroll(-30.0, TouchPhase::Moved, clock.at(16));
        assert!(response.consumed && response.started);
        assert!(lift.is_dragging());

        let mut lift = Lift::new();
        lift.scroll(0.0, TouchPhase::Started, clock.at(0));
        assert!(!lift.scroll(20.0, TouchPhase::Moved, clock.at(16)).consumed);
        assert!(!lift.scroll(-20.0, TouchPhase::Moved, clock.at(32)).consumed);
        assert!(!lift.is_dragging());
    }

    #[test]
    fn momentum_after_release_is_swallowed() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        let release = slow_drag(&mut lift, &clock, &[-90.0, -90.0]);
        lift.scroll(0.0, TouchPhase::Ended, clock.at(release));
        assert!(
            lift.scroll(-40.0, TouchPhase::Moved, clock.at(release + 16))
                .consumed
        );
        assert!(
            lift.scroll(0.0, TouchPhase::Ended, clock.at(release + 32))
                .consumed
        );
        assert!(
            !lift
                .scroll(0.0, TouchPhase::Ended, clock.at(release + 48))
                .consumed
        );
    }

    #[test]
    fn a_cancel_springs_back() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        let release = slow_drag(&mut lift, &clock, &[-200.0, -100.0]);
        let response = lift.scroll(0.0, TouchPhase::Cancelled, clock.at(release));
        assert_eq!(response.event, Some(LiftEvent::Close));
        settle(&mut lift, &clock, release);
        assert_eq!(lift.progress(), 0.0);
    }

    #[test]
    fn animate_and_snap_reach_either_end() {
        let clock = Clock(Instant::now());
        let mut lift = Lift::new();
        lift.animate(true, clock.at(0));
        assert!(lift.is_moving());
        let ms = settle(&mut lift, &clock, 16);
        assert_eq!(lift.progress(), 1.0);
        lift.animate(false, clock.at(ms));
        settle(&mut lift, &clock, ms + 16);
        assert_eq!(lift.progress(), 0.0);
        lift.snap(true);
        assert_eq!(lift.progress(), 1.0);
        assert!(!lift.is_moving());
        lift.animate(false, clock.at(0));
        lift.finish();
        assert!(!lift.tick(clock.at(1)));
        assert_eq!(lift.progress(), 0.0);
    }
}
