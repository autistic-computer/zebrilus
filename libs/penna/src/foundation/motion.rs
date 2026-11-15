use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use web_time::Duration;
pub use web_time::Instant;

use ala::{
    Animation, AnimationElement, AnimationExt as _, App, ElementId, Hsla, IntoElement, Length,
    ParentElement as _, RenderOnce, Styled, Transformation, Window, div, ease_out_quint,
    prelude::FluentBuilder as _, px, radians,
};

use crate::{
    Icon, IconName,
    foundation::{SURFACE_ENTER_DURATION, SURFACE_LEAVE_DURATION},
};

pub const FAST_ANIMATION_SPEED: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    Off,
    #[default]
    Default,
    Fast,
}

impl Motion {
    pub const ALL: [Self; 3] = [Self::Off, Self::Default, Self::Fast];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Default => "Default",
            Self::Fast => "Fast",
        }
    }

    pub const fn reduced(self) -> bool {
        matches!(self, Self::Off)
    }

    pub const fn speed(self) -> f32 {
        match self {
            Self::Fast => FAST_ANIMATION_SPEED,
            Self::Off | Self::Default => 1.0,
        }
    }
}

pub(crate) const TURN: Duration = Duration::from_millis(220);
const ENTER: Duration = Duration::from_millis(200);
const ENTER_DROP: f32 = 8.0;
pub(crate) const APPEAR: Duration = Duration::from_millis(120);
pub(crate) const SPIN: Duration = Duration::from_millis(900);

#[derive(Default)]
struct Turns(HashMap<ElementId, (bool, Option<Instant>)>);

impl ala::Global for Turns {}

pub fn turned(id: impl Into<ElementId>, on: bool, cx: &mut App) -> Option<Instant> {
    let now = cx.background_executor().now();
    let entry = cx
        .default_global::<Turns>()
        .0
        .entry(id.into())
        .or_insert((on, None));
    if entry.0 != on {
        *entry = (on, Some(now));
    }
    entry.1
}

pub fn eased(
    since: Option<Instant>,
    delay: Duration,
    span: Duration,
    window: &Window,
    cx: &App,
) -> f32 {
    let Some(since) = since else {
        return 1.0;
    };
    let now = cx.background_executor().now();
    let elapsed = ala::animation_time(now.saturating_duration_since(since)).saturating_sub(delay);
    let t = (elapsed.as_secs_f32() / span.as_secs_f32()).clamp(0.0, 1.0);
    if t < 1.0 && !cx.reduce_motion() {
        window.request_animation_frame();
        ease_out_quint()(t)
    } else {
        1.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Presence {
    open: bool,
    since: Option<Instant>,
}

impl Presence {
    #[must_use]
    pub const fn is_open(self) -> bool {
        self.open
    }

    pub fn set(&mut self, open: bool, now: Instant) {
        if self.open != open {
            *self = Self {
                open,
                since: Some(now),
            };
        }
    }

    #[must_use]
    pub fn at(self, now: Instant) -> Option<f32> {
        let Some(since) = self.since else {
            return self.open.then_some(1.0);
        };
        let elapsed = ala::animation_time(now.saturating_duration_since(since)).as_secs_f32();
        if self.open {
            let t = elapsed / SURFACE_ENTER_DURATION.as_secs_f32();
            return Some(ease_out_quint()(t.min(1.0)));
        }
        let t = elapsed / SURFACE_LEAVE_DURATION.as_secs_f32();
        (t < 1.0).then(|| 1.0 - accelerate(t))
    }

    #[must_use]
    pub fn shown(self, window: &Window, cx: &App) -> Option<f32> {
        if cx.reduce_motion() {
            return self.open.then_some(1.0);
        }
        let shown = self.at(cx.background_executor().now());
        if shown.is_some_and(|shown| shown < 1.0 || !self.open) {
            window.request_animation_frame();
        }
        shown
    }

    #[must_use]
    pub fn mounted(self, cx: &App) -> bool {
        self.open || (!cx.reduce_motion() && self.at(cx.background_executor().now()).is_some())
    }
}

fn accelerate(t: f32) -> f32 {
    let curve = |a: f32, b: f32, s: f32| {
        let rest = 1.0 - s;
        3.0 * a * rest * rest * s + 3.0 * b * rest * s * s + s * s * s
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = f32::midpoint(low, high);
        if curve(0.4, 1.0, mid) < t {
            low = mid;
        } else {
            high = mid;
        }
    }
    curve(0.0, 1.0, f32::midpoint(low, high))
}

#[derive(IntoElement)]
pub struct Disclosure {
    id: ElementId,
    open: bool,
    size: Length,
    color: Option<Hsla>,
}

impl Disclosure {
    pub fn new(id: impl Into<ElementId>, open: bool, size: impl Into<Length>) -> Self {
        Self {
            id: id.into(),
            open,
            size: size.into(),
            color: None,
        }
    }

    #[must_use]
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for Disclosure {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let turn = eased(
            turned(self.id, self.open, cx),
            Duration::ZERO,
            TURN,
            window,
            cx,
        );
        let open = if self.open { turn } else { 1.0 - turn };
        Icon::new(IconName::ChevronRight)
            .size(self.size)
            .when_some(self.color, Styled::text_color)
            .transform(Transformation::rotate(radians(open * FRAC_PI_2)))
    }
}

pub fn enter(id: impl Into<ElementId>, row: impl IntoElement) -> impl IntoElement {
    div().relative().child(row).with_animation(
        id,
        Animation::new(ENTER).with_easing(ease_out_quint()),
        |row, shown| row.top(px(-ENTER_DROP * (1.0 - shown))).opacity(shown),
    )
}

pub fn enter_since<E: Styled>(row: E, since: Option<Instant>, window: &Window, cx: &App) -> E {
    let shown = eased(since, Duration::ZERO, ENTER, window, cx);
    if shown >= 1.0 {
        return row;
    }
    row.top(px(-ENTER_DROP * (1.0 - shown))).opacity(shown)
}

pub fn appear<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    control: E,
) -> AnimationElement<E> {
    control.with_animation(id, Animation::new(APPEAR), |control, shown| {
        control.opacity(shown)
    })
}

#[cfg(test)]
mod tests {
    use super::{Duration, Instant, Presence};

    #[test]
    fn a_closed_surface_stays_mounted_for_its_exit_and_reopening_cancels_it() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut presence = Presence::default();
        assert_eq!(presence.at(at(0)), None);
        presence.set(true, at(0));
        presence.set(false, at(200));
        assert_eq!(presence.at(at(200)), Some(1.0));
        assert!(presence.at(at(299)).is_some_and(|shown| shown > 0.0));
        assert_eq!(presence.at(at(300)), None);
        presence.set(true, at(250));
        assert_eq!(presence.at(at(250)), Some(0.0));
        assert_eq!(presence.at(at(300)).map(|shown| shown > 0.0), Some(true));
        assert_eq!(presence.at(at(410)), Some(1.0));
    }
}
