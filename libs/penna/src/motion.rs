use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use web_time::Duration;
pub use web_time::Instant;

use ala::{
    Animation, AnimationElement, AnimationExt as _, App, ElementId, Hsla, IntoElement, Length,
    ParentElement as _, RenderOnce, Styled, Transformation, Window, div, ease_out_quint,
    prelude::FluentBuilder as _, px, radians,
};

use crate::{Icon, IconName};

const TURN: Duration = Duration::from_millis(220);
const ENTER: Duration = Duration::from_millis(200);
const ENTER_DROP: f32 = 8.0;
const APPEAR: Duration = Duration::from_millis(120);

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
