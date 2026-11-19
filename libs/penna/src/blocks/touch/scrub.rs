use std::{cell::Cell, rc::Rc};

use ala::{
    AnyElement, App, Bounds, Context, Div, Entity, EventEmitter, HitboxBehavior, IntoElement,
    LongPressEvent, ParentElement as _, Pixels, Render, ScrollWheelEvent, SharedString, Stateful,
    Styled as _, TouchDragEvent, TouchPhase, Window, canvas, div, prelude::*, px,
};
use web_time::Duration;

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Space, StyledExt as _, TextStyle, Tone,
};

pub const SCRUB_HOLD: Duration = Duration::from_millis(300);
pub const SCRUB_STEP_MIN: f32 = 28.0;
const CARD_WIDTH: f32 = 168.0;
const CARD_HEIGHT: f32 = 304.0;
const CARD_RADIUS: f32 = 28.0;
const SIDE_WIDTH: f32 = 64.0;
const SIDE_HEIGHT: f32 = 116.0;
const SIDE_RADIUS: f32 = 14.0;
const SIDE_GAP: f32 = 8.0;
const SIDE_STEP: f32 = 72.0;
const SIDE_INK: f32 = 0.6;
const EDGE: f32 = 16.0;
const RING: f32 = 2.0;
const GLYPH: f32 = 16.0;
const LIFT: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrubEvent {
    Began(usize),
    Moved(usize),
    Ended(usize),
    Cancelled,
}

#[must_use]
pub fn scrub_index(start: usize, from: Pixels, at: Pixels, step: Pixels, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let steps = f32::from(at - from) / f32::from(step.max(px(1.0)));
    let moved = steps.round() as isize;
    (start as isize + moved).clamp(0, count as isize - 1) as usize
}

#[must_use]
pub fn scrub_step(span: Pixels, count: usize) -> Pixels {
    (span / count.max(1) as f32).max(px(SCRUB_STEP_MIN))
}

pub struct PillScrub {
    count: usize,
    current: usize,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    pressed: bool,
    from: Pixels,
    picked: Option<usize>,
}

impl EventEmitter<ScrubEvent> for PillScrub {}

impl PillScrub {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            count: 0,
            current: 0,
            bounds: Rc::default(),
            pressed: false,
            from: px(0.0),
            picked: None,
        }
    }

    pub fn set_pages(&mut self, count: usize, current: usize) {
        self.count = count;
        self.current = current.min(count.saturating_sub(1));
        if let Some(picked) = self.picked {
            self.picked = Some(picked.min(count.saturating_sub(1)));
        }
    }

    pub fn picked(&self) -> Option<usize> {
        self.picked
    }

    fn press(&mut self) {
        self.pressed = true;
    }

    fn pass(&mut self) {
        self.pressed = false;
    }

    fn begin(&mut self, at: Pixels, cx: &mut Context<Self>) -> bool {
        if !self.pressed || self.count < 2 {
            return false;
        }
        self.pressed = false;
        self.from = at;
        self.picked = Some(self.current);
        cx.haptic(ala::Haptic::Selection);
        cx.emit(ScrubEvent::Began(self.current));
        cx.notify();
        true
    }

    fn follow(&mut self, at: Pixels, cx: &mut Context<Self>) {
        let Some(picked) = self.picked else {
            return;
        };
        let step = scrub_step(self.bounds.get().size.width, self.count);
        let next = scrub_index(self.current, self.from, at, step, self.count);
        if next != picked {
            self.picked = Some(next);
            cx.haptic(ala::Haptic::Selection);
            cx.emit(ScrubEvent::Moved(next));
            cx.notify();
        }
    }

    fn end(&mut self, cx: &mut Context<Self>) {
        if let Some(picked) = self.picked.take() {
            cx.emit(ScrubEvent::Ended(picked));
            cx.notify();
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.pressed = false;
        if self.picked.take().is_some() {
            cx.emit(ScrubEvent::Cancelled);
            cx.notify();
        }
    }

    pub fn anchor(scrub: &Entity<Self>, cx: &App) -> impl IntoElement + use<> {
        let bounds = Rc::clone(&scrub.read(cx).bounds);
        let scrub = scrub.downgrade();
        canvas(
            move |area, window, _| {
                bounds.set(area);
                window.insert_hitbox(area, HitboxBehavior::Normal)
            },
            move |_, hitbox, window, _| {
                window.on_mouse_event({
                    let scrub = scrub.clone();
                    let hitbox = hitbox.clone();
                    move |event: &TouchDragEvent, phase, window, cx| {
                        if !phase.bubble()
                            || event.phase != TouchPhase::Started
                            || window.default_prevented()
                            || !hitbox.is_hovered_at(event.start_position, window)
                        {
                            return;
                        }
                        let Some(scrub) = scrub.upgrade() else {
                            return;
                        };
                        window.set_long_press_delay(SCRUB_HOLD, cx);
                        scrub.update(cx, |scrub, _| scrub.press());
                    }
                });
                window.on_mouse_event({
                    let scrub = scrub.clone();
                    let hitbox = hitbox.clone();
                    move |event: &LongPressEvent, phase, window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        let Some(scrub) = scrub.upgrade() else {
                            return;
                        };
                        match event.phase {
                            TouchPhase::Started => {
                                if window.default_prevented()
                                    || !hitbox.is_hovered_at(event.start_position, window)
                                {
                                    return;
                                }
                                let began =
                                    scrub.update(cx, |scrub, cx| scrub.begin(event.position.x, cx));
                                if began {
                                    window.prevent_default();
                                    window.capture_long_press(&scrub);
                                }
                            }
                            _ if !window.has_long_press_capture(&scrub) => {}
                            TouchPhase::Moved => scrub.update(cx, |scrub, cx| {
                                scrub.follow(event.position.x, cx);
                            }),
                            TouchPhase::Ended => scrub.update(cx, Self::end),
                            TouchPhase::Cancelled => scrub.update(cx, Self::cancel),
                        }
                    }
                });
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
                    if phase.bubble()
                        || event.touch_phase != TouchPhase::Started
                        || !hitbox.bounds.contains(&event.position)
                    {
                        return;
                    }
                    if let Some(scrub) = scrub.upgrade() {
                        scrub.update(cx, |scrub, _| scrub.pass());
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }
}

impl Render for PillScrub {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct ScrubCard {
    pub icon: IconName,
    pub title: SharedString,
    pub tone: Tone,
    pub preview: AnyElement,
}

pub fn scrub_cards(
    cards: Vec<ScrubCard>,
    picked: usize,
    under: Pixels,
    floor: Pixels,
    heading: Option<SharedString>,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let width = window.viewport_size().width;
    let half = px(CARD_WIDTH / 2.0);
    let middle = under.clamp(
        px(EDGE) + half,
        (width - px(EDGE) - half).max(px(EDGE) + half),
    );
    let ink = theme.foreground;
    let muted = ink.muted();
    let warning = theme.warning;
    let mut heading = heading;
    div()
        .id("pill-scrub")
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom(floor)
        .occlude()
        .bg(theme.scrim)
        .children(cards.into_iter().enumerate().map(|(index, card)| {
            let offset = index as isize - picked as isize;
            let on = offset == 0;
            let (w, h, radius) = if on {
                (CARD_WIDTH, CARD_HEIGHT, CARD_RADIUS)
            } else {
                (SIDE_WIDTH, SIDE_HEIGHT, SIDE_RADIUS)
            };
            let center = if on {
                middle
            } else {
                let away = CARD_WIDTH / 2.0
                    + SIDE_GAP
                    + SIDE_WIDTH / 2.0
                    + (offset.unsigned_abs() - 1) as f32 * SIDE_STEP;
                middle + px(away * offset.signum() as f32)
            };
            let tone_ink = if card.tone == Tone::Neutral {
                None
            } else {
                Some(warning)
            };
            let name = div()
                .flex()
                .items_center()
                .gap(Space::S6)
                .min_w_0()
                .text_color(tone_ink.unwrap_or(if on { ink } else { muted }))
                .child(Icon::new(card.icon).size(px(GLYPH)).flex_none())
                .when(on, |name| {
                    name.child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .type_style_strong(TextStyle::TouchSmall, cx)
                            .child(card.title.clone()),
                    )
                });
            let shot = div()
                .relative()
                .flex()
                .flex_col()
                .w(px(w))
                .h(px(h))
                .overflow_hidden()
                .rounded(px(radius))
                .bg(theme.background)
                .when(on, |shot| shot.border(px(RING)).border_color(ink))
                .child(card.preview);
            div()
                .absolute()
                .left(center - px(w / 2.0))
                .bottom(px(LIFT))
                .w(px(w))
                .flex()
                .flex_col()
                .gap(Space::S8)
                .when(!on, |card| card.items_center().opacity(SIDE_INK))
                .when(on, |card| {
                    card.children(heading.take().map(|heading| {
                        div()
                            .type_style_strong(TextStyle::Caption, cx)
                            .text_color(muted)
                            .child(heading)
                    }))
                })
                .child(name)
                .child(shot)
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scrub_steps_one_pane_per_step_from_where_it_was_held() {
        let step = scrub_step(px(226.0), 5);
        assert_eq!(step, px(45.2));
        assert_eq!(scrub_index(0, px(150.0), px(150.0), step, 5), 0);
        assert_eq!(scrub_index(0, px(150.0), px(172.0), step, 5), 0);
        assert_eq!(scrub_index(0, px(150.0), px(173.0), step, 5), 1);
        assert_eq!(scrub_index(0, px(150.0), px(286.0), step, 5), 3);
        assert_eq!(scrub_index(2, px(150.0), px(60.0), step, 5), 0);
        assert_eq!(scrub_index(0, px(150.0), px(900.0), step, 5), 4);
        assert_eq!(scrub_step(px(226.0), 20), px(SCRUB_STEP_MIN));
    }
}
