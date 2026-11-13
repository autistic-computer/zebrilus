use ala::{
    Anchor, AnyElement, App, Context, EventEmitter, IntoElement, ParentElement as _, Pixels, Point,
    Render, Styled as _, Task, Window, anchored, deferred, div, point, prelude::*, px, relative,
};
use web_time::Duration;

use super::{
    key_press::{Grip, Press, press_listeners},
    key_row::{KEY_ICON, KEY_SIZE, KeyLook, key_surface},
};
use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, Kind, Layer, PaintState, Space,
    StyledExt as _, Surface, Tone, rems_from_px,
};

const THRESHOLD: f32 = 10.0;
const REPEAT_DELAY: Duration = Duration::from_millis(300);
const REPEAT_INTERVAL: Duration = Duration::from_millis(80);
const CELL: f32 = 44.0;
const GLYPH: f32 = 20.0;
const GROW: f32 = 0.04;
const HUB: f32 = 6.0;
const WINDOW_MARGIN: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ArrowDirection {
    pub fn key(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Up => IconName::ArrowUp,
            Self::Down => IconName::ArrowDown,
            Self::Left => IconName::ArrowLeft,
            Self::Right => IconName::ArrowRight,
        }
    }
}

pub fn resolve_direction(dx: f32, dy: f32, threshold: f32) -> Option<ArrowDirection> {
    if dx.hypot(dy) < threshold {
        return None;
    }
    Some(if dx.abs() >= dy.abs() {
        if dx < 0.0 {
            ArrowDirection::Left
        } else {
            ArrowDirection::Right
        }
    } else if dy < 0.0 {
        ArrowDirection::Up
    } else {
        ArrowDirection::Down
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowPadEvent {
    Arrow(ArrowDirection),
}

pub struct ArrowPad {
    press: Option<(Point<Pixels>, Grip)>,
    direction: Option<ArrowDirection>,
    repeat: Option<Task<()>>,
}

impl EventEmitter<ArrowPadEvent> for ArrowPad {}

impl ArrowPad {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            press: None,
            direction: None,
            repeat: None,
        }
    }

    pub fn held(&self) -> bool {
        self.press.is_some()
    }

    pub fn direction(&self) -> Option<ArrowDirection> {
        self.direction
    }

    fn repeat(direction: ArrowDirection, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |pad, cx| {
            let mut wait = REPEAT_DELAY;
            loop {
                cx.background_executor().timer(wait).await;
                let held = pad.update(cx, |pad, cx| {
                    let held = pad.direction == Some(direction);
                    if held {
                        cx.emit(ArrowPadEvent::Arrow(direction));
                    }
                    held
                });
                if !matches!(held, Ok(true)) {
                    break;
                }
                wait = REPEAT_INTERVAL;
            }
        })
    }

    fn bubble(&self, cx: &App) -> AnyElement {
        arrow_pad_bubble(self.direction, cx)
    }
}

pub fn arrow_pad_bubble(direction: Option<ArrowDirection>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let cell_size = px(CELL);
    let cell_corner = if theme.pill_controls() {
        cell_size / 2.0
    } else {
        theme.control_corner(cell_size)
    };
    let inset = Space::S6;
    let floating = theme.layer(Surface::new(Kind::Floating.role()), PaintState::Rest);
    let layer = Layer {
        radius: cell_corner + px(inset.design_px()),
        share: None,
        ..floating.clone()
    };
    let muted = theme
        .paint_on(floating.ground, Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest);
    let solid = theme.paint(Tone::Primary, Emphasis::Solid);
    let reduced = cx.reduce_motion();
    let cell = |slot: Option<ArrowDirection>, hub: bool| {
        let active = slot.is_some() && slot == direction;
        let grown = if active && !reduced { CELL * GROW } else { 0.0 };
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(CELL + grown))
            .m(px(-grown / 2.0))
            .rounded(cell_corner * (1.0 + grown / CELL))
            .when_some(slot, |cell, direction| {
                let selector = format!("arrow-pad-cell-{}", direction.key());
                cell.debug_selector(move || selector)
                    .map(|cell| {
                        if active {
                            cell.bg(solid.fill).text_color(solid.text)
                        } else {
                            cell.text_color(muted)
                        }
                    })
                    .child(Icon::new(direction.icon()).size(px(GLYPH)))
            })
            .when(hub, |cell| {
                cell.child(
                    div()
                        .size(px(HUB))
                        .rounded_full()
                        .bg(theme.foreground.muted()),
                )
            })
    };
    let line = |cells: [AnyElement; 3]| div().flex().gap(Space::S4).children(cells);
    let blank = || cell(None, false).into_any_element();
    div()
        .debug_selector(|| "arrow-pad-bubble".to_owned())
        .flex()
        .flex_col()
        .p(inset)
        .gap(Space::S4)
        .layer(&layer)
        .child(line([
            blank(),
            cell(Some(ArrowDirection::Up), false).into_any_element(),
            blank(),
        ]))
        .child(line([
            cell(Some(ArrowDirection::Left), false).into_any_element(),
            cell(None, true).into_any_element(),
            cell(Some(ArrowDirection::Right), false).into_any_element(),
        ]))
        .child(line([
            blank(),
            cell(Some(ArrowDirection::Down), false).into_any_element(),
            blank(),
        ]))
        .into_any_element()
}

fn popover_above(card: impl IntoElement, window: &Window) -> impl IntoElement {
    let gap = Space::S8.rems().to_pixels(window.rem_size());
    div().absolute().top_0().left(relative(0.5)).child(
        deferred(
            anchored()
                .anchor(Anchor::BottomCenter)
                .offset(point(px(0.0), -gap))
                .snap_to_window_with_margin(px(WINDOW_MARGIN))
                .child(card),
        )
        .with_priority(2),
    )
}

impl Press for ArrowPad {
    fn grip(&self) -> Option<Grip> {
        self.press.map(|(_, grip)| grip)
    }

    fn grab(&mut self, at: Point<Pixels>, grip: Grip, cx: &mut Context<Self>) {
        self.press = Some((at, grip));
        self.direction = None;
        self.repeat = None;
        cx.notify();
    }

    fn track(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((origin, _)) = self.press else {
            return;
        };
        let direction = resolve_direction(
            f32::from(at.x - origin.x),
            f32::from(at.y - origin.y),
            THRESHOLD,
        );
        if direction == self.direction {
            return;
        }
        self.direction = direction;
        self.repeat = direction.map(|direction| {
            cx.selection_haptic();
            cx.emit(ArrowPadEvent::Arrow(direction));
            Self::repeat(direction, cx)
        });
        cx.notify();
    }

    fn release(&mut self, _cancelled: bool, cx: &mut Context<Self>) {
        if self.press.take().is_some() {
            self.direction = None;
            self.repeat = None;
            cx.notify();
        }
    }
}

impl Render for ArrowPad {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let held = self.held();
        let look = if held {
            KeyLook::Inverted
        } else {
            KeyLook::Rest
        };
        key_surface(div().id("arrow-pad"), look, cx)
            .debug_selector(|| "arrow-pad".to_owned())
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(rems_from_px(KEY_SIZE))
            .child(Icon::new(IconName::ArrowsMove).size(rems_from_px(KEY_ICON)))
            .child(press_listeners(cx.entity().downgrade()))
            .when(held, |pad| {
                pad.child(popover_above(self.bubble(cx), window))
            })
    }
}

#[cfg(test)]
mod tests {
    use ala::{
        Entity, MouseButton, Subscription, TestAppContext, TouchDragEvent, TouchPhase,
        VisualTestContext, point, px,
    };

    use super::*;

    #[test]
    fn direction_follows_the_dominant_axis_past_the_threshold() {
        assert_eq!(resolve_direction(3.0, 4.0, 10.0), None);
        assert_eq!(
            resolve_direction(12.0, 3.0, 10.0),
            Some(ArrowDirection::Right)
        );
        assert_eq!(
            resolve_direction(-12.0, 3.0, 10.0),
            Some(ArrowDirection::Left)
        );
        assert_eq!(
            resolve_direction(3.0, -12.0, 10.0),
            Some(ArrowDirection::Up)
        );
        assert_eq!(
            resolve_direction(-3.0, 12.0, 10.0),
            Some(ArrowDirection::Down)
        );
        assert_eq!(
            resolve_direction(8.0, 8.0, 10.0),
            Some(ArrowDirection::Right)
        );
        assert_eq!(ArrowDirection::Left.key(), "left");
        assert_eq!(ArrowDirection::Up.key(), "up");
    }

    struct Host {
        pad: Entity<ArrowPad>,
        arrows: Vec<ArrowDirection>,
        _subscription: Subscription,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .absolute()
                    .bottom(px(20.0))
                    .left(px(200.0))
                    .child(self.pad.clone()),
            )
        }
    }

    fn host(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (host, cx) = cx.add_window_view(|_, cx| {
            let pad = cx.new(ArrowPad::new);
            let subscription = cx.subscribe(&pad, |host: &mut Host, _, event, _| {
                let ArrowPadEvent::Arrow(direction) = event;
                host.arrows.push(*direction);
            });
            Host {
                pad,
                arrows: Vec::new(),
                _subscription: subscription,
            }
        });
        redraw(cx);
        (host, cx)
    }

    fn redraw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn touch(
        cx: &mut VisualTestContext,
        phase: TouchPhase,
        start: Point<Pixels>,
        at: Point<Pixels>,
    ) {
        cx.simulate_event(TouchDragEvent {
            phase,
            start_position: start,
            position: at,
        });
        redraw(cx);
    }

    fn arrows(host: &Entity<Host>, cx: &mut VisualTestContext) -> Vec<ArrowDirection> {
        host.read_with(cx, |host, _| host.arrows.clone())
    }

    #[ala::test]
    fn a_touch_drag_emits_arrows_and_repeats_while_held(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let pad = host.read_with(cx, |host, _| host.pad.clone());
        let center = cx.debug_bounds("arrow-pad").expect("pad").center();

        touch(cx, TouchPhase::Started, center, center);
        assert!(pad.read_with(cx, |pad, _| pad.held()));
        assert!(cx.debug_bounds("arrow-pad-bubble").is_some());
        assert!(arrows(&host, cx).is_empty());

        let right = center + point(px(20.0), px(2.0));
        touch(cx, TouchPhase::Moved, center, right);
        assert_eq!(arrows(&host, cx), [ArrowDirection::Right]);
        assert_eq!(
            pad.read_with(cx, |pad, _| pad.direction()),
            Some(ArrowDirection::Right)
        );
        let bubble = cx.debug_bounds("arrow-pad-bubble").expect("bubble");
        let key = cx.debug_bounds("arrow-pad").expect("pad");
        assert!(bubble.bottom() <= key.top());

        touch(
            cx,
            TouchPhase::Moved,
            center,
            right + point(px(4.0), px(0.0)),
        );
        assert_eq!(arrows(&host, cx).len(), 1);

        cx.executor()
            .advance_clock(REPEAT_DELAY.saturating_sub(Duration::from_millis(1)));
        assert_eq!(arrows(&host, cx).len(), 1);
        cx.executor().advance_clock(Duration::from_millis(1));
        assert_eq!(arrows(&host, cx).len(), 2);
        cx.executor().advance_clock(REPEAT_INTERVAL);
        assert_eq!(arrows(&host, cx).len(), 3);
        cx.executor().advance_clock(REPEAT_INTERVAL * 2);
        assert_eq!(arrows(&host, cx).len(), 5);

        let up = center + point(px(0.0), px(-20.0));
        touch(cx, TouchPhase::Moved, center, up);
        assert_eq!(arrows(&host, cx).last(), Some(&ArrowDirection::Up));
        assert_eq!(arrows(&host, cx).len(), 6);
        cx.executor().advance_clock(REPEAT_INTERVAL);
        assert_eq!(arrows(&host, cx).len(), 6);
        cx.executor()
            .advance_clock(REPEAT_DELAY.saturating_sub(REPEAT_INTERVAL));
        assert_eq!(arrows(&host, cx).len(), 7);

        touch(cx, TouchPhase::Ended, center, up);
        assert!(!pad.read_with(cx, |pad, _| pad.held()));
        assert!(cx.debug_bounds("arrow-pad-bubble").is_none());
        cx.executor().advance_clock(Duration::from_secs(2));
        assert_eq!(arrows(&host, cx).len(), 7);
    }

    #[ala::test]
    fn returning_to_the_center_stops_the_repeat(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let center = cx.debug_bounds("arrow-pad").expect("pad").center();
        touch(cx, TouchPhase::Started, center, center);
        touch(
            cx,
            TouchPhase::Moved,
            center,
            center + point(px(-30.0), px(0.0)),
        );
        assert_eq!(arrows(&host, cx), [ArrowDirection::Left]);
        touch(
            cx,
            TouchPhase::Moved,
            center,
            center + point(px(-2.0), px(0.0)),
        );
        cx.executor().advance_clock(Duration::from_secs(1));
        assert_eq!(arrows(&host, cx), [ArrowDirection::Left]);
        touch(cx, TouchPhase::Cancelled, center, center);
    }

    #[ala::test]
    fn a_touch_outside_the_pad_is_not_claimed(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let pad = host.read_with(cx, |host, _| host.pad.clone());
        let outside = point(px(10.0), px(10.0));
        touch(cx, TouchPhase::Started, outside, outside);
        assert!(!pad.read_with(cx, |pad, _| pad.held()));
    }

    #[ala::test]
    fn the_mouse_drives_the_pad_too(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let pad = host.read_with(cx, |host, _| host.pad.clone());
        let center = cx.debug_bounds("arrow-pad").expect("pad").center();
        cx.simulate_mouse_down(center, MouseButton::Left, ala::Modifiers::none());
        redraw(cx);
        assert!(pad.read_with(cx, |pad, _| pad.held()));
        cx.simulate_mouse_move(
            center + point(px(0.0), px(15.0)),
            Some(MouseButton::Left),
            ala::Modifiers::none(),
        );
        redraw(cx);
        assert_eq!(arrows(&host, cx), [ArrowDirection::Down]);
        cx.simulate_mouse_up(
            center + point(px(0.0), px(15.0)),
            MouseButton::Left,
            ala::Modifiers::none(),
        );
        redraw(cx);
        assert!(!pad.read_with(cx, |pad, _| pad.held()));
        cx.executor().advance_clock(Duration::from_secs(1));
        assert_eq!(arrows(&host, cx), [ArrowDirection::Down]);
    }
}
