use ala::{
    App, Bounds, Div, ElementId, FULL_CORNER_RADIUS, FontWeight, InteractiveElement as _,
    MouseButton, ParentElement as _, Pixels, SharedString, SpringConfig, SpringState, Stateful,
    Styled as _, TextRun, Window, div, point, px, size,
};
use web_time::Instant;

use super::bar::{COMPACT_PILL_HEIGHT, PillPlacement, floating_layer};
use crate::{
    ActiveTheme as _, Layer, PaintState, StyledExt as _, Surface, rems_from_px, roles,
    touch::{KEY_PILL_HEIGHT, KEY_ROW_HEIGHT, KEY_ROW_MARGIN_X, KEY_ROW_MARGIN_Y, KeyRow},
};

pub const COMPACT_MINI_PILL_HEIGHT: f32 = 32.0;

const SPRING: SpringConfig = SpringConfig::new(520.0, 46.0, 1.0);
const SETTLE_EPSILON: f32 = 0.002;
const FADE: f32 = 0.6;
const MINI_TEXT: f32 = 13.0;
const MINI_PADDING_X: f32 = 16.0;
const MINI_MIN_WIDTH: f32 = 88.0;
const MINI_DROP: f32 = 14.0;
const MINI_BOTTOM_MIN: f32 = 6.0;

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
    let progress = progress.clamp(0.0, 1.0);
    let between = |from: Pixels, to: Pixels| from + (to - from) * progress;
    let height = between(px(KEY_PILL_HEIGHT), px(COMPACT_PILL_HEIGHT));
    let top = between(px(KEY_ROW_MARGIN_Y), pill_top).min(floor - px(KEY_ROW_MARGIN_Y) - height);
    let middle = top + height / 2.0;
    let start = between(px(KEY_ROW_MARGIN_X + keys.lead_room()), placement.inset);
    let side = between(px(KEY_ROW_MARGIN_X), placement.inset);
    let pill = floating_layer(placement, cx);
    let keys_layer = cx
        .theme()
        .layer(Surface::new(roles::PHONE_PILL), PaintState::Rest);
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
                .left(start)
                .right(side)
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
                .left(placement.inset)
                .right(placement.inset)
                .flex()
                .opacity(fade((progress - (1.0 - FADE)) / FADE))
                .child(label),
        )
}

pub fn pill_bounds(
    progress: f32,
    width: Pixels,
    mini: Pixels,
    placement: &PillPlacement,
) -> Bounds<Pixels> {
    let progress = progress.clamp(0.0, 1.0);
    let between = |from: Pixels, to: Pixels| from + (to - from) * progress;
    let full = (width - placement.inset * 2.0).max(px(0.0));
    let mini_bottom = (placement.bottom - px(MINI_DROP)).max(px(MINI_BOTTOM_MIN));
    let pill_width = between(mini.min(full), full);
    let height = between(px(COMPACT_MINI_PILL_HEIGHT), px(COMPACT_PILL_HEIGHT));
    let bottom = between(mini_bottom, placement.bottom);
    Bounds::new(
        point(
            (width - pill_width) / 2.0,
            placement.zone() - bottom - height,
        ),
        size(pill_width, height),
    )
}

pub fn pill_corner(
    progress: f32,
    bounds: Bounds<Pixels>,
    placement: &PillPlacement,
    cx: &App,
) -> Pixels {
    let pill = floating_layer(placement, cx);
    if pill.radius >= FULL_CORNER_RADIUS {
        return FULL_CORNER_RADIUS;
    }
    if cx.theme().style(roles::PHONE_PILL).shape == crate::Shape::Square {
        return pill.radius;
    }
    let mini = px(COMPACT_MINI_PILL_HEIGHT / 2.0);
    let radius = mini + (pill.radius - mini) * progress.clamp(0.0, 1.0);
    if radius * 2.0 >= bounds.size.height - px(0.01) {
        FULL_CORNER_RADIUS
    } else {
        radius
    }
}

pub fn mini_pill_width(text: &SharedString, window: &Window, cx: &App) -> Pixels {
    let font_size = rems_from_px(MINI_TEXT).to_pixels(window.rem_size());
    let mut font = ala::font(cx.theme().font_family.clone());
    font.weight = FontWeight::MEDIUM;
    let run = TextRun {
        len: text.len(),
        font,
        color: cx.theme().foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(text.clone(), font_size, &[run], None);
    (line.width + px(MINI_PADDING_X * 2.0)).max(px(MINI_MIN_WIDTH))
}

pub fn compact_pill_collapse(
    id: impl Into<ElementId>,
    label: Div,
    short: SharedString,
    progress: f32,
    bounds: Bounds<Pixels>,
    width: Pixels,
    placement: &PillPlacement,
    cx: &App,
) -> Stateful<Div> {
    let progress = progress.clamp(0.0, 1.0);
    let pill = floating_layer(placement, cx);
    let pressed = cx
        .theme()
        .layer(Surface::new(roles::PHONE_PILL), PaintState::Active)
        .fill;
    let radius = pill_corner(progress, bounds, placement, cx);
    let full = (width - placement.inset * 2.0).max(px(0.0));
    let fade = |at: f32| {
        let at = at.clamp(0.0, 1.0);
        at * at * (3.0 - 2.0 * at)
    };
    div()
        .id(id)
        .debug_selector(|| "compact-bar-pill".to_owned())
        .absolute()
        .left(bounds.origin.x)
        .top(bounds.origin.y)
        .w(bounds.size.width)
        .h(bounds.size.height)
        .overflow_hidden()
        .font_family(cx.theme().font_family.clone())
        .layer(&Layer { radius, ..pill })
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(crate::touch::press_highlight(
            "compact-bar-pill-press",
            pressed,
            radius,
        ))
        .child(
            div()
                .absolute()
                .left((bounds.size.width - full) / 2.0)
                .top((bounds.size.height - px(COMPACT_PILL_HEIGHT)) / 2.0)
                .w(full)
                .h(px(COMPACT_PILL_HEIGHT))
                .flex()
                .opacity(fade((progress - (1.0 - FADE)) / FADE))
                .child(label),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .px(px(MINI_PADDING_X))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .text_size(rems_from_px(MINI_TEXT))
                .line_height(rems_from_px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(pill.text)
                .opacity(1.0 - fade(progress / FADE))
                .child(short),
        )
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
    fn the_small_pill_sits_centered_below_the_full_one() {
        let placement = PillPlacement::new(Some(px(62.0)), px(34.0));
        let width = px(402.0);
        let full = pill_bounds(1.0, width, px(120.0), &placement);
        assert_eq!(full.origin.x, placement.inset);
        assert_eq!(full.origin.y, placement.pill_top());
        assert_eq!(full.size.height, px(COMPACT_PILL_HEIGHT));

        let mini = pill_bounds(0.0, width, px(120.0), &placement);
        assert_eq!(mini.size, size(px(120.0), px(COMPACT_MINI_PILL_HEIGHT)));
        assert_eq!(mini.center().x, width / 2.0);
        assert_eq!(
            placement.zone() - mini.bottom(),
            placement.bottom - px(MINI_DROP)
        );
        assert!(mini.bottom() > full.bottom());

        let wide = pill_bounds(0.0, width, px(600.0), &placement);
        assert_eq!(wide.size.width, full.size.width);
    }
}
