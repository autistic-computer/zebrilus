use ala::{
    App, Bounds, Div, ElementId, FULL_CORNER_RADIUS, InteractiveElement as _, MouseButton,
    ParentElement as _, Pixels, Point, SpringConfig, SpringState, Stateful, Styled as _, div,
    point, px, size,
};
use web_time::Instant;

use super::bar::{BAR_DISC, BAR_GAP, COMPACT_PILL_HEIGHT, PillPlacement, floating_layer};
use crate::{
    ActiveTheme as _, Layer, PaintState, Space, StyledExt as _, Surface,
    blocks::touch::{
        KEY_PILL_HEIGHT, KEY_ROW_HEIGHT, KEY_ROW_MARGIN_X, KEY_ROW_MARGIN_Y, KeyRow, key_pill_layer,
    },
    foundation::roles,
};

const SPRING: SpringConfig = SpringConfig::new(520.0, 46.0, 1.0);
const SETTLE_EPSILON: f32 = 0.002;
const FADE: f32 = 0.6;

#[derive(Clone, Copy, Debug)]
pub struct PillSpring {
    state: SpringState,
    target: f32,
    at: Option<Instant>,
}

impl Default for PillSpring {
    fn default() -> Self {
        Self {
            state: SpringState {
                position: 1.0,
                velocity: 0.0,
            },
            target: 1.0,
            at: None,
        }
    }
}

impl PillSpring {
    pub fn shut() -> Self {
        Self {
            state: SpringState {
                position: 0.0,
                velocity: 0.0,
            },
            target: 0.0,
            at: None,
        }
    }

    pub fn progress(&self) -> f32 {
        self.state.position
    }

    pub fn full(&self) -> bool {
        self.target == 1.0
    }

    pub fn aim(&mut self, full: bool, now: Instant) {
        let target = if full { 1.0 } else { 0.0 };
        if target != self.target {
            self.target = target;
            self.at.get_or_insert(now);
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(at) = self.at else {
            return false;
        };
        let elapsed = now.saturating_duration_since(at).as_secs_f32();
        let state = SPRING.step(self.state, self.target, elapsed);
        if SPRING.is_settled(state, self.target, SETTLE_EPSILON) {
            self.finish();
            return false;
        }
        self.state = state;
        self.at = Some(now);
        true
    }

    pub fn finish(&mut self) {
        self.state = SpringState {
            position: self.target,
            velocity: 0.0,
        };
        self.at = None;
    }
}

pub fn compact_pill_morph(
    keys: KeyRow,
    label: Div,
    progress: f32,
    pill_top: Pixels,
    floor: Pixels,
    placement: &PillPlacement,
    cx: &App,
) -> Div {
    compact_pill_morph_between(
        keys,
        label,
        progress,
        pill_top,
        floor,
        (px(0.0), px(0.0)),
        placement,
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn compact_pill_morph_between(
    keys: KeyRow,
    label: Div,
    progress: f32,
    pill_top: Pixels,
    floor: Pixels,
    ends: (Pixels, Pixels),
    placement: &PillPlacement,
    cx: &App,
) -> Div {
    let progress = progress.clamp(0.0, 1.0);
    let between = |from: Pixels, to: Pixels| from + (to - from) * progress;
    let height = between(px(KEY_PILL_HEIGHT), px(COMPACT_PILL_HEIGHT));
    let top = between(px(KEY_ROW_MARGIN_Y), pill_top).min(floor - px(KEY_ROW_MARGIN_Y) - height);
    let middle = top + height / 2.0;
    let left = between(px(KEY_ROW_MARGIN_X), placement.inset + ends.0);
    let right = between(px(KEY_ROW_MARGIN_X), placement.inset + ends.1);
    let pill = floating_layer(placement, cx);
    let keys_layer = key_pill_layer(cx.theme());
    let half = |layer: &Layer, height: f32| layer.radius.min(px(height / 2.0));
    let radius = between(
        half(&keys_layer, KEY_PILL_HEIGHT),
        half(&pill, COMPACT_PILL_HEIGHT),
    );
    let radius = if radius * 2.0 >= height - px(0.01) {
        FULL_CORNER_RADIUS
    } else {
        radius
    };
    let fade = |at: f32| {
        let at = at.clamp(0.0, 1.0);
        at * at * (3.0 - 2.0 * at)
    };
    div()
        .relative()
        .size_full()
        .font_family(cx.theme().font_family.clone())
        .child(
            div()
                .absolute()
                .top(top)
                .left(left)
                .right(right)
                .h(height)
                .layer(&Layer { radius, ..pill }),
        )
        .child(
            div()
                .absolute()
                .top(middle - px(KEY_ROW_HEIGHT / 2.0))
                .left_0()
                .right_0()
                .opacity(1.0 - fade(progress / FADE))
                .child(keys.bare()),
        )
        .child(
            div()
                .absolute()
                .top(middle - px(COMPACT_PILL_HEIGHT / 2.0))
                .left(placement.inset + ends.0)
                .right(placement.inset + ends.1)
                .flex()
                .opacity(fade((progress - (1.0 - FADE)) / FADE))
                .child(label),
        )
}

pub fn pill_bounds(width: Pixels, placement: &PillPlacement) -> Bounds<Pixels> {
    let full = (width - placement.inset * 2.0).max(px(0.0));
    Bounds::new(
        point(
            placement.inset,
            placement.zone() - placement.bottom - px(COMPACT_PILL_HEIGHT),
        ),
        size(full, px(COMPACT_PILL_HEIGHT)),
    )
}

pub fn bar_ends(start: bool, end: bool) -> (Pixels, Pixels) {
    let step = px(BAR_DISC + BAR_GAP);
    (
        if start { step } else { px(0.0) },
        if end { step } else { px(0.0) },
    )
}

pub fn split_pill_bounds(
    width: Pixels,
    placement: &PillPlacement,
    start: bool,
    end: bool,
) -> Bounds<Pixels> {
    let full = pill_bounds(width, placement);
    let (left, right) = bar_ends(start, end);
    Bounds::new(
        point(full.origin.x + left, full.origin.y),
        size(
            (full.size.width - left - right).max(px(0.0)),
            full.size.height,
        ),
    )
}

pub fn bar_disc_bounds(width: Pixels, placement: &PillPlacement, end: bool) -> Bounds<Pixels> {
    let full = pill_bounds(width, placement);
    let x = if end {
        full.right() - px(BAR_DISC)
    } else {
        full.left()
    };
    Bounds::new(point(x, full.origin.y), size(px(BAR_DISC), px(BAR_DISC)))
}

#[must_use]
pub fn over_pill(pill: Bounds<Pixels>, size: Pixels) -> Point<Pixels> {
    let reach = ((px(crate::blocks::touch::TOUCH_TARGET) - pill.size.height) / 2.0).max(px(0.0));
    point(
        pill.center().x - size / 2.0,
        pill.top() - reach - px(Space::GAP_RELATED.design_px()) - size,
    )
}

pub fn pill_corner(bounds: Bounds<Pixels>, placement: &PillPlacement, cx: &App) -> Pixels {
    let radius = floating_layer(placement, cx).radius;
    if radius * 2.0 >= bounds.size.height - px(0.01) {
        FULL_CORNER_RADIUS
    } else {
        radius
    }
}

pub fn compact_pill_at(
    id: impl Into<ElementId>,
    label: Div,
    bounds: Bounds<Pixels>,
    placement: &PillPlacement,
    cx: &App,
) -> Stateful<Div> {
    let pill = floating_layer(placement, cx);
    let pressed = cx
        .theme()
        .layer(Surface::new(roles::PHONE_PILL), PaintState::Active)
        .fill;
    let radius = pill_corner(bounds, placement, cx);
    div()
        .id(id)
        .debug_selector(|| "compact-bar-pill".to_owned())
        .click_cursor()
        .absolute()
        .left(bounds.origin.x)
        .top(bounds.origin.y)
        .w(bounds.size.width)
        .h(bounds.size.height)
        .flex()
        .overflow_hidden()
        .font_family(cx.theme().font_family.clone())
        .layer(&Layer { radius, ..pill })
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(crate::blocks::touch::press_highlight(
            "compact-bar-pill-press",
            pressed,
            radius,
        ))
        .child(label)
}

#[cfg(test)]
mod tests {
    use web_time::Duration;

    use super::*;

    #[test]
    fn the_morph_springs_to_its_end_and_turns_back_midway() {
        let start = Instant::now();
        let mut morph = PillSpring::default();
        assert_eq!(morph.progress(), 1.0);
        assert!(!morph.tick(start));

        morph.aim(false, start);
        assert!(morph.tick(start + Duration::from_millis(60)));
        let midway = morph.progress();
        assert!(midway > 0.0 && midway < 1.0);

        morph.aim(true, start + Duration::from_millis(60));
        assert!(morph.tick(start + Duration::from_millis(76)));
        let mut at = start + Duration::from_millis(76);
        while morph.tick(at) {
            at += Duration::from_millis(16);
            assert!(at < start + Duration::from_secs(2));
        }
        assert_eq!(morph.progress(), 1.0);

        morph.aim(false, at);
        morph.finish();
        assert_eq!(morph.progress(), 0.0);
        assert!(!morph.tick(at));
    }

    #[test]
    fn the_pill_spans_the_bar_above_its_bottom_inset() {
        let placement = PillPlacement::new(Some(px(62.0)), px(34.0));
        let pill = pill_bounds(px(402.0), &placement);
        assert_eq!(pill.origin, point(placement.inset, placement.pill_top()));
        assert_eq!(
            pill.size,
            size(px(402.0) - placement.inset * 2.0, px(COMPACT_PILL_HEIGHT))
        );
    }
}
