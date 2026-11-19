use ala::{
    Context, EventEmitter, IntoElement, ParentElement as _, Pixels, Point, Render, Styled as _,
    Task, Window, div, prelude::*, px,
};
use web_time::{Duration, Instant};

use super::{
    key_press::{Grip, Press, press_listeners},
    key_row::{DISC_ICON, KEY_SIZE, KeyLook, key_surface},
};
use crate::{Icon, IconName, rems_from_px};

const THRESHOLD: f32 = 10.0;
const LEAN: f32 = 10.0;
const FULL_LEAN: f32 = 40.0;
const REPEAT_DELAY: Duration = Duration::from_millis(300);
const REPEAT_SLOW: Duration = Duration::from_millis(80);
const REPEAT_FAST: Duration = Duration::from_millis(40);
const SETTLE: Duration = Duration::from_millis(180);

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
            Self::Up => IconName::ChevronUp,
            Self::Down => IconName::ChevronDown,
            Self::Left => IconName::ChevronLeft,
            Self::Right => IconName::ChevronRight,
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

fn lean(dx: f32, dy: f32) -> (f32, f32) {
    let distance = dx.hypot(dy);
    if distance <= LEAN {
        return (dx, dy);
    }
    let scale = LEAN / distance;
    (dx * scale, dy * scale)
}

fn repeat_interval(distance: f32) -> Duration {
    let at = ((distance - THRESHOLD) / (FULL_LEAN - THRESHOLD)).clamp(0.0, 1.0);
    REPEAT_SLOW.mul_f32(1.0 - at) + REPEAT_FAST.mul_f32(at)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowPadEvent {
    Arrow(ArrowDirection),
}

pub struct ArrowPad {
    press: Option<(Point<Pixels>, Grip)>,
    reach: (f32, f32),
    direction: Option<ArrowDirection>,
    repeat: Option<Task<()>>,
    settle: Option<((f32, f32), Instant)>,
}

impl EventEmitter<ArrowPadEvent> for ArrowPad {}

impl ArrowPad {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            press: None,
            reach: (0.0, 0.0),
            direction: None,
            repeat: None,
            settle: None,
        }
    }

    pub fn held(&self) -> bool {
        self.press.is_some()
    }

    pub fn direction(&self) -> Option<ArrowDirection> {
        self.direction
    }

    fn send(direction: ArrowDirection, cx: &mut Context<Self>) {
        cx.haptic(ala::Haptic::Selection);
        cx.emit(ArrowPadEvent::Arrow(direction));
    }

    fn repeat(direction: ArrowDirection, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |pad, cx| {
            let mut wait = REPEAT_DELAY;
            loop {
                cx.background_executor().timer(wait).await;
                let next = pad.update(cx, |pad, cx| {
                    (pad.direction == Some(direction)).then(|| {
                        Self::send(direction, cx);
                        repeat_interval(pad.reach.0.hypot(pad.reach.1))
                    })
                });
                match next {
                    Ok(Some(interval)) => wait = interval,
                    _ => break,
                }
            }
        })
    }

    fn knob(&self, now: Instant, reduced: bool) -> ((f32, f32), bool) {
        if reduced {
            return ((0.0, 0.0), false);
        }
        if self.press.is_some() {
            return (lean(self.reach.0, self.reach.1), false);
        }
        let Some(((x, y), since)) = self.settle else {
            return ((0.0, 0.0), false);
        };
        let at = (now.saturating_duration_since(since).as_secs_f32() / SETTLE.as_secs_f32())
            .clamp(0.0, 1.0);
        let left = (1.0 - at).powi(3);
        ((x * left, y * left), at < 1.0)
    }
}

impl Press for ArrowPad {
    fn grip(&self) -> Option<Grip> {
        self.press.map(|(_, grip)| grip)
    }

    fn grab(&mut self, at: Point<Pixels>, grip: Grip, cx: &mut Context<Self>) {
        self.press = Some((at, grip));
        self.reach = (0.0, 0.0);
        self.direction = None;
        self.repeat = None;
        self.settle = None;
        cx.notify();
    }

    fn track(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((origin, _)) = self.press else {
            return;
        };
        self.reach = (f32::from(at.x - origin.x), f32::from(at.y - origin.y));
        let direction = resolve_direction(self.reach.0, self.reach.1, THRESHOLD);
        if direction != self.direction {
            self.direction = direction;
            self.repeat = direction.map(|direction| {
                Self::send(direction, cx);
                Self::repeat(direction, cx)
            });
        }
        cx.notify();
    }

    fn release(&mut self, _cancelled: bool, cx: &mut Context<Self>) {
        if self.press.take().is_some() {
            self.settle = Some((lean(self.reach.0, self.reach.1), Instant::now()));
            self.reach = (0.0, 0.0);
            self.direction = None;
            self.repeat = None;
            cx.notify();
        }
    }
}

impl Render for ArrowPad {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let held = self.held();
        let ((x, y), settling) = self.knob(Instant::now(), cx.reduce_motion());
        if settling {
            window.request_animation_frame();
        } else if !held {
            self.settle = None;
        }
        let look = if held { KeyLook::On } else { KeyLook::Rest };
        let glyph = self
            .direction
            .map_or(IconName::ArrowsMove, ArrowDirection::icon);
        div().relative().w_full().h(rems_from_px(KEY_SIZE)).child(
            key_surface(div().id("arrow-pad"), look, cx)
                .debug_selector(|| "arrow-pad".to_owned())
                .absolute()
                .left(px(x))
                .top(px(y))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .w_full()
                .h(rems_from_px(KEY_SIZE))
                .child(Icon::new(glyph).size(rems_from_px(DISC_ICON)))
                .child(press_listeners(cx.entity().downgrade())),
        )
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
    fn the_knob_leans_and_repeats_faster_the_further_the_finger_goes(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let pad = host.read_with(cx, |host, _| host.pad.clone());
        let rest = cx.debug_bounds("arrow-pad").expect("pad");
        let center = rest.center();

        touch(cx, TouchPhase::Started, center, center);
        assert!(pad.read_with(cx, |pad, _| pad.held()));
        assert!(arrows(&host, cx).is_empty());

        touch(
            cx,
            TouchPhase::Moved,
            center,
            center + point(px(40.0), px(0.0)),
        );
        assert_eq!(arrows(&host, cx), [ArrowDirection::Right]);
        let leaning = cx.debug_bounds("arrow-pad").expect("pad");
        assert_eq!(leaning.origin - rest.origin, point(px(10.0), px(0.0)));

        cx.executor()
            .advance_clock(REPEAT_DELAY.saturating_sub(Duration::from_millis(1)));
        assert_eq!(arrows(&host, cx).len(), 1);
        cx.executor().advance_clock(Duration::from_millis(1));
        assert_eq!(arrows(&host, cx).len(), 2);
        cx.executor().advance_clock(REPEAT_FAST);
        assert_eq!(arrows(&host, cx).len(), 3);

        touch(
            cx,
            TouchPhase::Moved,
            center,
            center + point(px(10.0), px(0.0)),
        );
        assert_eq!(arrows(&host, cx).len(), 3);
        cx.executor().advance_clock(REPEAT_FAST);
        assert_eq!(arrows(&host, cx).len(), 4);
        cx.executor()
            .advance_clock(REPEAT_SLOW.saturating_sub(Duration::from_millis(1)));
        assert_eq!(arrows(&host, cx).len(), 4);
        cx.executor().advance_clock(Duration::from_millis(1));
        assert_eq!(arrows(&host, cx).len(), 5);

        touch(
            cx,
            TouchPhase::Moved,
            center,
            center + point(px(0.0), px(-20.0)),
        );
        assert_eq!(arrows(&host, cx).last(), Some(&ArrowDirection::Up));
        assert_eq!(arrows(&host, cx).len(), 6);

        touch(
            cx,
            TouchPhase::Ended,
            center,
            center + point(px(0.0), px(-20.0)),
        );
        assert!(!pad.read_with(cx, |pad, _| pad.held()));
        cx.executor().advance_clock(Duration::from_secs(2));
        assert_eq!(arrows(&host, cx).len(), 6);
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
