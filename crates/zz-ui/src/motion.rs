//! Small motions the chrome shares: a disclosure chevron that turns as its
//! group opens and closes, rows that settle into place as they appear, and
//! controls that fade in as the pointer reaches their row.

use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use web_time::{Duration, Instant};

use zz_gpui::{
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

impl zz_gpui::Global for Turns {}

/// When the two-state control `id` last turned to `on`, or `None` if it has
/// not turned since it was first drawn.
pub(crate) fn turned(id: impl Into<ElementId>, on: bool, cx: &mut App) -> Option<Instant> {
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

/// How far a motion `span` long, starting `delay` after `since`, has come,
/// eased: 1 once it has finished, when it never started, or while motion is
/// reduced. Asks for the next frame while it runs.
pub(crate) fn eased(
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
    let t = (now.saturating_duration_since(since + delay).as_secs_f32() / span.as_secs_f32())
        .clamp(0.0, 1.0);
    if t < 1.0 && !cx.reduce_motion() {
        window.request_animation_frame();
        ease_out_quint()(t)
    } else {
        1.0
    }
}

/// A chevron pointing right while its group is closed and down while it is
/// open, turning between the two as the group opens and closes. `id` names
/// the group, so the turn survives the row being rebuilt. Without
/// [`Self::color`] it takes the text color around it.
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

/// `row` fading in and dropping into place over the first frames it is drawn
/// under `id`: a row that just appeared, or one whose group just opened.
pub fn enter(id: impl Into<ElementId>, row: impl IntoElement) -> impl IntoElement {
    div().relative().child(row).with_animation(
        id,
        Animation::new(ENTER).with_easing(ease_out_quint()),
        |row, shown| row.top(px(-ENTER_DROP * (1.0 - shown))).opacity(shown),
    )
}

/// `control` fading in over the first frames it is drawn under `id`, for a
/// control that only shows while the pointer is over its row.
pub fn appear<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    control: E,
) -> AnimationElement<E> {
    control.with_animation(id, Animation::new(APPEAR), |control, shown| {
        control.opacity(shown)
    })
}
