use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use ala::{
    AnyElement, App, Bounds, Context, Div, ElementId, Entity, EventEmitter, FULL_CORNER_RADIUS,
    HitboxBehavior, Hsla, IntoElement, KeyDownEvent, LongPressEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Render, Role,
    ScrollWheelEvent, SharedString, Stateful, Styled as _, Task, TextRun, TouchDragEvent,
    TouchPhase, Window, canvas, div, prelude::*, px,
};
use web_time::{Duration, Instant};

use super::{
    key_press::Grip,
    stack_gesture::{STACK_HOLD_MS, StackGesture, StackSlide, StackTarget, StackTouch},
};
use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, Kind, Layer, PaintState, Smoothing,
    Space, StyledExt as _, Surface, TextStyle, Theme, Tone,
    blocks::compact::PillSpring,
    foundation::{author_name, motion::APPEAR},
};

pub const STACK_CHIP_HEIGHT: f32 = 52.0;
pub const STACK_HOLD: Duration = Duration::from_millis(STACK_HOLD_MS);
const GLYPH: f32 = 20.0;
const LABEL_HEIGHT: f32 = 28.0;
const CAP_WIDTH: f32 = 32.0;
const CLOSE_SIZE: f32 = 56.0;
const CLOSE_ICON: f32 = 22.0;
const LEAD: f32 = 16.0;
const TRAIL: f32 = 20.0;
const LABEL_INSET: f32 = 12.0;
const EDGE: f32 = 16.0;
const RISE: f32 = 14.0;
const STAGGER: f32 = 0.06;
const GROW: f32 = 0.04;
const FOLD_SCALE: f32 = 0.5;
const ONE_COLUMN: usize = 6;
const ROW_MAX: usize = 3;
const FLASH: Duration = Duration::from_millis(120);
const CHIP_GAP: f32 = Space::GAP_RELATED.design_px();
const GROUP_GAP: f32 = Space::GAP_GROUP.design_px();
const MORE: &str = "stack-more";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StackLook {
    #[default]
    Rest,
    Quiet,
    Danger,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StackChip {
    pub id: SharedString,
    pub label: SharedString,
    pub icon: Option<IconName>,
    pub chord: Option<SharedString>,
    pub value: Option<SharedString>,
    pub look: StackLook,
    pub disabled: bool,
    pub place: bool,
}

impl StackChip {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        icon: IconName,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: Some(icon),
            chord: None,
            value: None,
            look: StackLook::Rest,
            disabled: false,
            place: false,
        }
    }

    pub fn chord(
        id: impl Into<SharedString>,
        chord: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            chord: Some(chord.into()),
            icon: None,
            ..Self::new(id, label, IconName::Keyboard)
        }
    }

    #[must_use]
    pub fn look(mut self, look: StackLook) -> Self {
        self.look = look;
        self
    }

    #[must_use]
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn place(mut self) -> Self {
        self.place = true;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Shape {
    #[default]
    Column,
    Row,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StackGroup {
    label: Option<SharedString>,
    chips: Vec<StackChip>,
    shape: Shape,
}

impl StackGroup {
    pub fn new(chips: impl IntoIterator<Item = StackChip>) -> Self {
        Self {
            label: None,
            chips: chips.into_iter().collect(),
            shape: Shape::Column,
        }
    }

    pub fn row(chips: impl IntoIterator<Item = StackChip>) -> Self {
        Self {
            label: None,
            chips: chips.into_iter().take(ROW_MAX).collect(),
            shape: Shape::Row,
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    fn chords(&self) -> bool {
        !self.chips.is_empty() && self.chips.iter().all(|chip| chip.chord.is_some())
    }

    fn columns(&self) -> usize {
        match self.shape {
            Shape::Row => self.chips.len().max(1),
            Shape::Column if self.chips.len() > ONE_COLUMN => 2,
            Shape::Column => 1,
        }
    }

    fn rows(&self) -> usize {
        match self.shape {
            Shape::Row => 1,
            Shape::Column => self.chips.len().div_ceil(self.columns()),
        }
    }

    fn height(&self) -> f32 {
        let rows = self.rows() as f32;
        let label = if self.label.is_some() {
            LABEL_HEIGHT + CHIP_GAP
        } else {
            0.0
        };
        rows * STACK_CHIP_HEIGHT + (rows - 1.0).max(0.0) * CHIP_GAP + label
    }
}

fn fitting(groups: &[StackGroup], cap: usize, room: f32) -> usize {
    let height = |shown: usize, more: bool| {
        let groups: f32 = groups[..shown].iter().map(StackGroup::height).sum();
        let parts = shown + usize::from(more);
        groups
            + if more { STACK_CHIP_HEIGHT } else { 0.0 }
            + parts.saturating_sub(1) as f32 * GROUP_GAP
    };
    let most = cap.min(groups.len());
    (1..=most)
        .rev()
        .find(|&shown| height(shown, shown < groups.len()) <= room)
        .unwrap_or(usize::from(groups.len() == 1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackClose {
    Disc,
    Anchor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackMenuEvent {
    Pick(SharedString),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Center,
    Start,
    End,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    Chip(SharedString, bool),
    More,
}

#[derive(Default)]
struct Measured {
    chips: Vec<Bounds<Pixels>>,
    labels: Vec<Bounds<Pixels>>,
    width: Pixels,
    layer: Option<Bounds<Pixels>>,
}

fn layer_bounds(measured: &Measured, window: &Window) -> Bounds<Pixels> {
    measured.layer.unwrap_or_else(|| Bounds {
        origin: Point::default(),
        size: window.viewport_size(),
    })
}

fn track_layer(measured: Rc<RefCell<Measured>>) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let mut measured = measured.borrow_mut();
            if measured.layer != Some(bounds) {
                measured.layer = Some(bounds);
                window.refresh();
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
}

struct Frame {
    progress: f32,
    trail: f32,
    reduced: bool,
    anchor: Bounds<Pixels>,
    now: Instant,
}

pub struct StackMenu {
    groups: Vec<StackGroup>,
    fold_at: Option<usize>,
    start: usize,
    shown_groups: usize,
    close: StackClose,
    own_scrim: bool,
    gesture: StackGesture,
    grip: Option<Grip>,
    hold: Option<Task<()>>,
    shown: bool,
    spring: PillSpring,
    since: Option<Instant>,
    progress: f32,
    pinned: bool,
    picked: Option<(usize, Instant)>,
    trail: PillSpring,
    over_since: Option<Instant>,
    lifting: Option<SharedString>,
    anchor: Rc<Cell<Bounds<Pixels>>>,
    measured: Rc<RefCell<Measured>>,
    targets: Vec<Target>,
}

impl EventEmitter<StackMenuEvent> for StackMenu {}

impl StackMenu {
    pub fn new(close: StackClose, _: &mut Context<Self>) -> Self {
        Self {
            groups: Vec::new(),
            fold_at: None,
            start: 0,
            shown_groups: 0,
            close,
            own_scrim: true,
            gesture: StackGesture::default(),
            grip: None,
            hold: None,
            shown: false,
            spring: PillSpring::shut(),
            since: None,
            progress: 0.0,
            pinned: false,
            picked: None,
            trail: PillSpring::shut(),
            over_since: None,
            lifting: None,
            anchor: Rc::default(),
            measured: Rc::default(),
            targets: Vec::new(),
        }
    }

    #[must_use]
    pub fn scrim_apart(mut self) -> Self {
        self.own_scrim = false;
        self
    }

    pub fn set_groups(&mut self, groups: Vec<StackGroup>, cx: &mut Context<Self>) {
        if self.groups != groups {
            self.groups = groups;
            cx.notify();
        }
    }

    pub fn set_fold(&mut self, fold_at: Option<usize>, cx: &mut Context<Self>) {
        if self.fold_at != fold_at {
            self.fold_at = fold_at;
            cx.notify();
        }
    }

    pub fn has_groups(&self) -> bool {
        self.groups.iter().any(|group| !group.chips.is_empty())
    }

    pub fn is_open(&self) -> bool {
        self.gesture.is_open()
    }

    pub fn is_held(&self) -> bool {
        self.gesture.is_held()
    }

    pub fn is_pressed(&self) -> bool {
        self.gesture.is_pressed()
    }

    pub fn is_shown(&self) -> bool {
        self.shown || self.progress > 0.0 || self.picked.is_some()
    }

    pub fn close_shown(&self) -> f32 {
        if self.pinned && self.close == StackClose::Disc {
            self.progress.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn track_anchor(&self) -> impl IntoElement + use<> {
        let anchor = Rc::clone(&self.anchor);
        canvas(move |bounds, _, _| anchor.set(bounds), |_, (), _, _| {})
            .absolute()
            .inset_0()
    }

    pub fn open(&mut self, cx: &mut Context<Self>) {
        if self.gesture.open() == StackTouch::Opened {
            self.pinned = true;
            self.appear(cx);
        }
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.gesture.close();
        self.hold = None;
        self.grip = None;
        self.disappear(cx);
    }

    pub fn press(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> StackTouch {
        cx.notify();
        self.gesture.press(at)
    }

    pub fn hold(&mut self, cx: &mut Context<Self>) -> StackTouch {
        let touch = self.gesture.hold();
        self.react(touch, cx);
        touch
    }

    pub fn pass(&mut self, cx: &mut Context<Self>) {
        self.hold = None;
        if self.gesture.pass() == StackTouch::Passed {
            cx.notify();
        }
    }

    pub fn track(
        &mut self,
        at: Point<Pixels>,
        slide: StackSlide,
        cx: &mut Context<Self>,
    ) -> StackTouch {
        let touch = self.gesture.track(at, slide);
        if touch == StackTouch::Passed {
            self.hold = None;
        }
        self.react(touch, cx);
        touch
    }

    pub fn release(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> StackTouch {
        self.hold = None;
        self.grip = None;
        let touch = self.gesture.release(at);
        self.react(touch, cx);
        touch
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.hold = None;
        self.grip = None;
        let touch = self.gesture.cancel();
        self.react(touch, cx);
    }

    pub fn lift(&mut self, id: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.lifting = Some(id.into());
        if !self.gesture.is_held() {
            self.gesture.close();
            self.gesture.press(self.anchor.get().center());
            let touch = self.gesture.hold();
            self.pinned = false;
            self.react(touch, cx);
        }
        cx.notify();
    }

    fn settle_lift(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.lifting.clone() else {
            return;
        };
        let Some(index) = self
            .targets
            .iter()
            .position(|target| matches!(target, Target::Chip(chip, _) if *chip == id))
        else {
            return;
        };
        let Some(bounds) = self
            .measured
            .borrow()
            .chips
            .get(index)
            .copied()
            .filter(|bounds| !bounds.is_empty())
        else {
            return;
        };
        self.lifting = None;
        let touch = self.gesture.track(bounds.center(), StackSlide::Pass);
        self.react(touch, cx);
    }

    fn react(&mut self, touch: StackTouch, cx: &mut Context<Self>) {
        match touch {
            StackTouch::Opened => {
                cx.selection_haptic();
                self.appear(cx);
            }
            StackTouch::Entered(_) => {
                cx.selection_haptic();
                self.over_since = Some(Instant::now());
                cx.notify();
            }
            StackTouch::Left => {
                self.over_since = None;
                cx.notify();
            }
            StackTouch::Picked(index) => self.pick(index, cx),
            StackTouch::Kept => {
                self.pinned = true;
                self.over_since = None;
                cx.notify();
            }
            StackTouch::Closed => self.disappear(cx),
            StackTouch::Idle | StackTouch::Tap | StackTouch::Passed => {}
        }
    }

    fn appear(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if !self.is_shown() {
            self.start = 0;
            self.measured.borrow_mut().chips.clear();
        }
        self.shown = true;
        self.picked = None;
        self.over_since = None;
        self.since = Some(now);
        self.spring.aim(true, now);
        self.trail = PillSpring::shut();
        cx.notify();
    }

    fn disappear(&mut self, cx: &mut Context<Self>) {
        if !self.shown {
            return;
        }
        let now = Instant::now();
        self.shown = false;
        self.since = Some(now);
        self.spring.aim(false, now);
        self.over_since = None;
        cx.notify();
    }

    fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        match self.targets.get(index).cloned() {
            Some(Target::More) => {
                self.start += self.shown_groups;
                self.measured.borrow_mut().chips.clear();
                self.gesture.open();
                self.pinned = true;
                self.over_since = None;
                cx.selection_haptic();
                cx.notify();
            }
            Some(Target::Chip(id, true)) => {
                self.gesture.close();
                let now = Instant::now();
                self.picked = Some((index, now));
                self.trail = PillSpring::default();
                self.disappear(cx);
                cx.emit(StackMenuEvent::Pick(id));
            }
            Some(Target::Chip(_, false)) | None => self.disappear(cx),
        }
    }

    fn pick_tap(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.gesture.is_open() {
            self.pick(index, cx);
        }
    }

    fn start_hold(&mut self, cx: &mut Context<Self>) {
        self.hold = Some(cx.spawn(async move |menu, cx| {
            cx.background_executor().timer(STACK_HOLD).await;
            let _ = menu.update(cx, |menu, cx| {
                menu.hold(cx);
            });
        }));
    }

    pub fn pill_anchor(menu: &Entity<Self>, cx: &App) -> impl IntoElement + use<> {
        let anchor = Rc::clone(&menu.read(cx).anchor);
        let menu = menu.downgrade();
        canvas(
            move |bounds, window, _| {
                anchor.set(bounds);
                window.insert_hitbox(bounds, HitboxBehavior::Normal)
            },
            move |_, hitbox, window, _| {
                window.on_mouse_event({
                    let menu = menu.clone();
                    let hitbox = hitbox.clone();
                    move |event: &TouchDragEvent, phase, window, cx| {
                        if !phase.bubble()
                            || event.phase != TouchPhase::Started
                            || window.default_prevented()
                            || !hitbox.is_hovered_at(event.start_position, window)
                        {
                            return;
                        }
                        let Some(menu) = menu.upgrade() else {
                            return;
                        };
                        if menu.read(cx).is_open() {
                            return;
                        }
                        window.set_long_press_delay(STACK_HOLD, cx);
                        menu.update(cx, |menu, cx| {
                            menu.grip = Some(Grip::Touch);
                            menu.press(event.start_position, cx);
                        });
                    }
                });
                window.on_mouse_event({
                    let menu = menu.clone();
                    let hitbox = hitbox.clone();
                    move |event: &LongPressEvent, phase, window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        let Some(menu) = menu.upgrade() else {
                            return;
                        };
                        match event.phase {
                            TouchPhase::Started => {
                                if window.default_prevented()
                                    || !hitbox.is_hovered_at(event.start_position, window)
                                    || !menu.read(cx).is_pressed()
                                {
                                    return;
                                }
                                window.prevent_default();
                                window.capture_long_press(&menu);
                                menu.update(cx, |menu, cx| {
                                    menu.hold(cx);
                                });
                            }
                            _ if !window.has_long_press_capture(&menu) => {}
                            TouchPhase::Moved => menu.update(cx, |menu, cx| {
                                menu.track(event.position, StackSlide::Pass, cx);
                            }),
                            TouchPhase::Ended => menu.update(cx, |menu, cx| {
                                menu.release(event.position, cx);
                            }),
                            TouchPhase::Cancelled => menu.update(cx, Self::cancel),
                        }
                    }
                });
                window.on_mouse_event({
                    let menu = menu.clone();
                    let hitbox = hitbox.clone();
                    move |event: &ScrollWheelEvent, phase, _, cx| {
                        if phase.bubble()
                            || event.touch_phase != TouchPhase::Started
                            || !hitbox.bounds.contains(&event.position)
                        {
                            return;
                        }
                        if let Some(menu) = menu.upgrade() {
                            menu.update(cx, Self::pass);
                        }
                    }
                });
                window.on_mouse_event({
                    let menu = menu.clone();
                    move |event: &MouseDownEvent, phase, window, cx| {
                        if !phase.bubble()
                            || event.button != MouseButton::Left
                            || !hitbox.is_hovered(window)
                        {
                            return;
                        }
                        let Some(menu) = menu.upgrade() else {
                            return;
                        };
                        menu.update(cx, |menu, cx| {
                            if menu.grip.is_some() || menu.is_open() {
                                return;
                            }
                            menu.grip = Some(Grip::Mouse);
                            menu.press(event.position, cx);
                            menu.start_hold(cx);
                        });
                    }
                });
                window.on_mouse_event({
                    let menu = menu.clone();
                    move |event: &MouseMoveEvent, phase, _, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        if let Some(menu) = menu.upgrade() {
                            menu.update(cx, |menu, cx| {
                                if menu.grip == Some(Grip::Mouse) {
                                    menu.track(event.position, StackSlide::Pass, cx);
                                }
                            });
                        }
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if !phase.bubble() || event.button != MouseButton::Left {
                        return;
                    }
                    if let Some(menu) = menu.upgrade() {
                        menu.update(cx, |menu, cx| {
                            if menu.grip.is_some() {
                                menu.release(event.position, cx);
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }

    fn frame(&mut self, window: &mut Window, cx: &App) -> Frame {
        let now = Instant::now();
        let reduced = cx.reduce_motion();
        let mut moving = self.spring.tick(now);
        if let Some((_, at)) = self.picked
            && now.saturating_duration_since(at) >= FLASH
        {
            self.trail.aim(false, at + FLASH);
            moving |= self.trail.tick(now) || self.trail.full();
        } else if self.picked.is_some() {
            moving = true;
        }
        let progress = if reduced {
            let faded = self.since.map_or(1.0, |since| {
                now.saturating_duration_since(since).as_secs_f32() / APPEAR.as_secs_f32()
            });
            moving |= faded < 1.0;
            let faded = faded.clamp(0.0, 1.0);
            if self.shown { faded } else { 1.0 - faded }
        } else {
            self.spring.progress()
        };
        let trail = if reduced {
            progress
        } else {
            self.trail.progress()
        };
        if let Some(since) = self.over_since
            && now.saturating_duration_since(since) < APPEAR
        {
            moving = true;
        }
        if moving {
            window.request_animation_frame();
        }
        self.progress = progress.max(0.0);
        let folded = !self.shown && progress <= 0.0 && (self.picked.is_none() || trail <= 0.0);
        if folded && !moving {
            self.picked = None;
            self.pinned = false;
            self.start = 0;
            self.progress = 0.0;
        }
        Frame {
            progress,
            trail,
            reduced,
            anchor: self.anchor.get(),
            now,
        }
    }
}

struct Parts<'a> {
    frame: &'a Frame,
    measured: Vec<Bounds<Pixels>>,
    labels: Vec<Bounds<Pixels>>,
    span: f32,
    shown: bool,
    picked: Option<usize>,
    over: Option<(usize, f32)>,
}

impl Parts<'_> {
    fn motion(&self, seq: usize, bounds: Option<Bounds<Pixels>>, picked: bool) -> Motion {
        let frame = self.frame;
        if self.shown {
            let shown = if frame.reduced {
                frame.progress
            } else {
                (frame.progress * self.span - STAGGER * seq as f32).clamp(0.0, 1.0)
            };
            let rise = if frame.reduced {
                0.0
            } else {
                RISE * (1.0 - shown)
            };
            return Motion {
                offset: ala::point(px(0.0), px(rise)),
                scale: 1.0,
                opacity: shown.clamp(0.0, 1.0),
            };
        }
        let left = if picked { frame.trail } else { frame.progress };
        if frame.reduced {
            return Motion {
                offset: Point::default(),
                scale: 1.0,
                opacity: left.clamp(0.0, 1.0),
            };
        }
        let fold = (1.0 - left).clamp(0.0, 1.0);
        let offset = bounds
            .filter(|bounds| !bounds.is_empty())
            .map_or_else(Point::default, |bounds| {
                (frame.anchor.center() - bounds.center()) * fold
            });
        Motion {
            offset,
            scale: 1.0 - FOLD_SCALE * fold,
            opacity: 1.0 - fold,
        }
    }

    fn grow(&self, target: usize) -> f32 {
        match self.over {
            Some((over, grown)) if over == target => 1.0 + GROW * grown,
            _ => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    offset: Point<Pixels>,
    scale: f32,
    opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    Plain,
    Chord,
    Glyph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fill {
    Rest,
    Inverted,
}

fn control_layer(layer: Layer, height: Pixels, theme: &Theme) -> Layer {
    if theme.pill_controls() {
        Layer {
            radius: FULL_CORNER_RADIUS,
            share: None,
            smoothing: Smoothing::CIRCLE.exponent(),
            ..layer
        }
    } else {
        Layer {
            radius: theme.control_corner(height),
            share: Some(theme.control_roundness()),
            ..layer
        }
    }
}

fn floating(height: f32, theme: &Theme) -> Layer {
    control_layer(
        theme.layer(Surface::new(Kind::Floating.role()), PaintState::Rest),
        px(height),
        theme,
    )
}

fn scaled(style: TextStyle, scale: f32) -> (ala::Rems, ala::Rems) {
    (
        ala::Rems(style.size().0 * scale),
        ala::Rems(style.line_height().0 * scale),
    )
}

fn sized<E: ala::Styled>(element: E, style: TextStyle, scale: f32) -> E {
    let (size, line) = scaled(style, scale);
    element.text_size(size).line_height(line)
}

fn moved<E: ala::Styled>(element: E, motion: Motion, natural: Pixels, height: f32) -> E {
    let scale = motion.scale;
    let element = element
        .relative()
        .left(motion.offset.x)
        .top(motion.offset.y)
        .opacity(motion.opacity);
    if (scale - 1.0).abs() < f32::EPSILON {
        return element;
    }
    let element = element.my(px(-(scale - 1.0) * height / 2.0));
    if natural > px(0.0) {
        element
            .w(natural * scale)
            .mx(-(natural * (scale - 1.0)) / 2.0)
    } else {
        element
    }
}

fn cut_middle(text: &SharedString, max: Pixels, window: &Window, cx: &App) -> SharedString {
    let style = TextStyle::TouchBody;
    let size = style.size().to_pixels(window.rem_size());
    let mut font = ala::font(cx.theme().font_family.clone());
    font.weight = style.strong_weight();
    let width = |text: &str| {
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: cx.theme().foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(SharedString::from(text.to_owned()), size, &[run], None)
            .width
    };
    if width(text) <= max {
        return text.clone();
    }
    let chars: Vec<char> = text.chars().collect();
    let cut = |keep: usize| {
        let head: String = chars[..keep.div_ceil(2)].iter().collect();
        let tail: String = chars[chars.len() - keep / 2..].iter().collect();
        format!("{head}\u{2026}{tail}")
    };
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        if width(&cut(middle)) <= max {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    cut(low).into()
}

pub fn stack_scrim(
    id: impl Into<ElementId>,
    progress: f32,
    layer: Bounds<Pixels>,
    window: &Window,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let floor =
        (window.visual_viewport_bounds().bottom() - layer.top()).clamp(px(0.0), layer.size.height);
    div()
        .id(id)
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(floor)
        .occlude()
        .bg(cx.theme().scrim)
        .opacity(progress.clamp(0.0, 1.0))
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |_, window, cx| on_close(window, cx))
}

impl StackMenu {
    pub fn scrim(menu: &Entity<Self>, window: &Window, cx: &App) -> impl IntoElement + use<> {
        let shut = menu.downgrade();
        let menu = menu.read(cx);
        let progress = menu.progress;
        let shown = menu.is_shown();
        let measured = Rc::clone(&menu.measured);
        let bounds = layer_bounds(&measured.borrow(), window);
        div().absolute().inset_0().when(shown, |layer| {
            layer.child(track_layer(measured)).child(stack_scrim(
                "stack-scrim",
                progress,
                bounds,
                window,
                move |_, cx| {
                    let _ = shut.update(cx, Self::close);
                },
                cx,
            ))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn chip(
        &self,
        chip: &StackChip,
        form: Form,
        target: usize,
        seq: usize,
        parts: &Parts<'_>,
        stretch: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let natural = parts.measured.get(target).copied().unwrap_or_default();
        let picked = parts.picked == Some(target);
        let under = parts.over.is_some_and(|(over, _)| over == target);
        let fill = if (picked || under) && !chip.disabled {
            Fill::Inverted
        } else {
            Fill::Rest
        };
        let mut motion = parts.motion(seq, Some(natural), picked);
        if !parts.frame.reduced {
            motion.scale *= parts.grow(target);
        }
        let scale = motion.scale;
        let height = STACK_CHIP_HEIGHT * scale;
        let rest = floating(STACK_CHIP_HEIGHT, theme);
        let solid = theme.paint(
            if chip.look == StackLook::Danger {
                Tone::Danger
            } else {
                Tone::Primary
            },
            Emphasis::Solid,
        );
        let neutral = theme.paint_on(rest.ground, Tone::Neutral, Emphasis::Ghost);
        let (layer, ink, muted) = match fill {
            Fill::Inverted => (
                Layer {
                    fill: solid.fill,
                    glass: None,
                    text: solid.text,
                    ..rest.clone()
                },
                solid.text,
                Hsla {
                    a: solid.text.a * 0.72,
                    ..solid.text
                },
            ),
            Fill::Rest => {
                let ink = if chip.disabled {
                    theme.foreground.faint()
                } else {
                    match chip.look {
                        StackLook::Rest => rest.text,
                        StackLook::Quiet => neutral.muted_at(PaintState::Rest),
                        StackLook::Danger => {
                            theme
                                .paint_on(rest.ground, Tone::Danger, Emphasis::Ghost)
                                .text
                        }
                    }
                };
                (rest.clone(), ink, neutral.muted_at(PaintState::Rest))
            }
        };
        let pressed = theme.paint(Tone::Primary, Emphasis::Solid);
        let id = ElementId::Name(format!("stack-chip-{}", chip.id).into());
        let author = author_name(&id);
        let glyph = |icon: IconName| Icon::new(icon).size(px(GLYPH * scale)).flex_none();
        let label = if chip.place {
            let room = layer_bounds(&self.measured.borrow(), window).size.width
                - px(2.0 * EDGE + LEAD + TRAIL + GLYPH)
                - Space::S10.rems().to_pixels(window.rem_size());
            cut_middle(&chip.label, room, window, cx)
        } else {
            chip.label.clone()
        };
        let face = div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .h(px(height))
            .whitespace_nowrap()
            .layer(&layer)
            .text_color(ink)
            .font_weight(TextStyle::TouchBody.strong_weight())
            .role(Role::Button)
            .accessibility_id(author)
            .aria_label(chip.label.clone())
            .aria_disabled(chip.disabled)
            .when(stretch, ala::Styled::w_full);
        let face = sized(face, TextStyle::TouchBody, scale).when(!chip.disabled, |face| {
            face.child(super::instant_press_highlight(
                ElementId::Name(format!("stack-press-{}", chip.id).into()),
                pressed.fill,
                match layer.share {
                    Some(share) => super::PressCorners::Share(share, layer.smoothing),
                    None => super::PressCorners::Adaptive(layer.radius),
                },
            ))
        });
        let face = match form {
            Form::Glyph => face
                .w(px(height))
                .justify_center()
                .children(chip.icon.clone().map(glyph)),
            Form::Plain | Form::Chord => face
                .pl(px(LEAD * scale))
                .pr(px(TRAIL * scale))
                .gap(px(Space::S10.design_px() * scale))
                .when(form == Form::Plain, |face| {
                    face.children(chip.icon.clone().map(glyph))
                })
                .when_some(
                    chip.chord.clone().filter(|_| form == Form::Chord),
                    |face, chord| {
                        face.child(
                            sized(
                                div()
                                    .flex_none()
                                    .min_w(px(CAP_WIDTH * scale))
                                    .font_family(theme.mono_font_family.clone())
                                    .font_weight(TextStyle::TouchSmall.weight())
                                    .text_color(muted),
                                TextStyle::TouchSmall,
                                scale,
                            )
                            .child(chord),
                        )
                    },
                )
                .child(div().min_w_0().overflow_hidden().child(label))
                .when_some(chip.value.clone(), |face, value| {
                    face.child(
                        sized(
                            div()
                                .flex_none()
                                .font_weight(TextStyle::TouchSmall.weight())
                                .text_color(muted),
                            TextStyle::TouchSmall,
                            scale,
                        )
                        .child(value),
                    )
                }),
        };
        let face = if chip.disabled {
            face
        } else {
            let on_pressed = pressed.text;
            face.click_cursor()
                .active(move |style| style.text_color(on_pressed))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.pick_tap(target, cx);
                }))
        };
        let face = moved(face, motion, natural.size.width, STACK_CHIP_HEIGHT);
        let measured = Rc::clone(&self.measured);
        div()
            .relative()
            .flex_none()
            .when(stretch, ala::Styled::w_full)
            .child(face)
            .child(
                canvas(
                    move |bounds, _, _| {
                        if let Some(slot) = measured.borrow_mut().chips.get_mut(target) {
                            *slot = bounds;
                        }
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    fn label(
        &self,
        text: SharedString,
        index: usize,
        seq: usize,
        parts: &Parts<'_>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let natural = parts.labels.get(index).copied().unwrap_or_default();
        let motion = parts.motion(seq, Some(natural), false);
        let layer = floating(LABEL_HEIGHT, theme);
        let muted = theme
            .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest);
        let face = div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(LABEL_HEIGHT * motion.scale))
            .px(px(LABEL_INSET * motion.scale))
            .whitespace_nowrap()
            .layer(&layer)
            .text_color(muted)
            .font_weight(TextStyle::TouchSmall.strong_weight())
            .child(text);
        let face = sized(face, TextStyle::TouchSmall, motion.scale);
        let face = moved(face, motion, natural.size.width, LABEL_HEIGHT);
        let measured = Rc::clone(&self.measured);
        div()
            .relative()
            .flex_none()
            .child(face)
            .child(
                canvas(
                    move |bounds, _, _| {
                        if let Some(slot) = measured.borrow_mut().labels.get_mut(index) {
                            *slot = bounds;
                        }
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    fn close_disc(
        &self,
        anchor: Bounds<Pixels>,
        area: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let solid = theme.paint(Tone::Primary, Emphasis::Solid);
        let size = px(CLOSE_SIZE);
        let layer = control_layer(
            Layer {
                fill: solid.fill,
                glass: None,
                text: solid.text,
                ..theme.layer(Surface::new(Kind::Floating.role()), PaintState::Rest)
            },
            size,
            theme,
        );
        let center = anchor.center() - area.origin;
        div()
            .id("stack-close")
            .absolute()
            .left(center.x - size / 2.0)
            .top(center.y - size / 2.0)
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .layer(&Layer {
                shadow: Vec::new(),
                ..layer
            })
            .opacity(self.close_shown())
            .click_cursor()
            .role(Role::Button)
            .accessibility_id(SharedString::new_static("stack-close"))
            .aria_label(SharedString::new_static("Close"))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .child(Icon::new(IconName::Xmark).size(px(CLOSE_ICON)))
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close(cx);
            }))
            .into_any_element()
    }
}

impl Render for StackMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = self.frame(window, cx);
        if !self.is_shown() {
            self.targets.clear();
            return div().into_any_element();
        }
        let layer = layer_bounds(&self.measured.borrow(), window);
        let viewport = layer.size;
        let anchor = frame.anchor;
        let top = window.fully_visible_bounds().top().max(layer.top()) + px(EDGE);
        let room = f32::from(anchor.top() - px(CHIP_GAP) - top);
        let start = self.start.min(self.groups.len());
        let rest = &self.groups[start..];
        let cap = if start == 0 {
            self.fold_at.unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };
        let shown_groups = fitting(rest, cap, room);
        let more = shown_groups < rest.len();
        let groups: Vec<StackGroup> = rest[..shown_groups].to_vec();
        self.shown_groups = shown_groups;
        let mut targets: Vec<Target> = groups
            .iter()
            .flat_map(|group| &group.chips)
            .map(|chip| Target::Chip(chip.id.clone(), !chip.disabled))
            .collect();
        if more {
            targets.push(Target::More);
        }
        let label_count = groups.iter().filter(|group| group.label.is_some()).count();
        let (measured, labels, width) = {
            let mut store = self.measured.borrow_mut();
            store.chips.resize(targets.len(), Bounds::default());
            store.labels.resize(label_count, Bounds::default());
            (store.chips.clone(), store.labels.clone(), store.width)
        };
        self.gesture.set_layout(
            anchor,
            measured
                .iter()
                .zip(&targets)
                .map(|(bounds, target)| StackTarget {
                    bounds: *bounds,
                    enabled: !bounds.is_empty() && !matches!(target, Target::Chip(_, false)),
                })
                .collect(),
            labels.clone(),
        );
        self.targets = targets;
        self.settle_lift(cx);
        let over = self.gesture.over().map(|over| {
            let grown = self.over_since.map_or(1.0, |since| {
                (frame.now.saturating_duration_since(since).as_secs_f32() / APPEAR.as_secs_f32())
                    .clamp(0.0, 1.0)
            });
            (over, 1.0 - (1.0 - grown).powi(3))
        });
        let part_count = self.targets.len() + label_count;
        let parts = Parts {
            frame: &frame,
            measured,
            labels,
            span: 1.0 + STAGGER * part_count.saturating_sub(1) as f32,
            shown: self.shown,
            picked: self.picked.map(|(index, _)| index),
            over,
        };
        let edge = px(EDGE);
        let axis = anchor.center().x - layer.left();
        let mut left = axis - width / 2.0;
        let mut align = Align::Center;
        if left < edge {
            left = edge;
            align = Align::Start;
        } else if left + width > viewport.width - edge {
            left = (viewport.width - edge - width).max(edge);
            align = Align::End;
        }
        let aligned = |element: Div| match align {
            Align::Center => element.items_center(),
            Align::Start => element.items_start(),
            Align::End => element.items_end(),
        };
        let mut target = 0;
        let mut seq = 0;
        let mut label_index = 0;
        let mut rendered: Vec<AnyElement> = Vec::new();
        for group in &groups {
            let first = target;
            target += group.chips.len();
            let chords = group.chords();
            let columns = group.columns();
            let form = match group.shape {
                Shape::Row => Form::Glyph,
                Shape::Column if chords => Form::Chord,
                Shape::Column => Form::Plain,
            };
            let stretch = chords || columns > 1 && group.shape == Shape::Column;
            let mut chips: Vec<(usize, AnyElement)> = group
                .chips
                .iter()
                .enumerate()
                .map(|(offset, chip)| {
                    let element = self.chip(
                        chip,
                        form,
                        first + offset,
                        seq + offset,
                        &parts,
                        stretch,
                        window,
                        cx,
                    );
                    (offset, element)
                })
                .collect();
            seq += group.chips.len();
            let label = group.label.clone().map(|text| {
                let element = self.label(text, label_index, seq, &parts, cx);
                label_index += 1;
                seq += 1;
                element
            });
            let body = match (group.shape, columns) {
                (Shape::Row, _) => div()
                    .flex()
                    .gap(Space::GAP_RELATED)
                    .children(chips.into_iter().map(|(_, chip)| chip)),
                (Shape::Column, 1) => {
                    chips.reverse();
                    let column = div()
                        .flex()
                        .flex_col()
                        .gap(Space::GAP_RELATED)
                        .children(chips.into_iter().map(|(_, chip)| chip));
                    if chords {
                        column.items_stretch()
                    } else {
                        aligned(column)
                    }
                }
                (Shape::Column, columns) => {
                    let mut rows: Vec<Vec<AnyElement>> = Vec::new();
                    for (offset, chip) in chips {
                        if offset % columns == 0 {
                            rows.push(Vec::new());
                        }
                        if let Some(row) = rows.last_mut() {
                            row.push(chip);
                        }
                    }
                    rows.reverse();
                    div()
                        .grid()
                        .grid_cols(columns as u16)
                        .gap(Space::GAP_RELATED)
                        .children(rows.into_iter().flatten())
                }
            };
            rendered.push(
                aligned(div().flex().flex_col().gap(Space::GAP_RELATED))
                    .children(label)
                    .child(body)
                    .into_any_element(),
            );
        }
        if more {
            let chip = StackChip::new(MORE, "More", IconName::Ellipsis);
            rendered.push(self.chip(&chip, Form::Plain, target, seq, &parts, false, window, cx));
        }
        rendered.reverse();
        let store = Rc::clone(&self.measured);
        let column = aligned(
            div()
                .absolute()
                .left(left)
                .bottom(layer.bottom() - anchor.top() + px(CHIP_GAP))
                .max_w(viewport.width - edge * 2.0)
                .flex()
                .flex_col()
                .gap(Space::GAP_GROUP),
        )
        .font_family(cx.theme().font_family.clone())
        .children(rendered)
        .child(
            canvas(
                move |bounds, window, _| {
                    let mut store = store.borrow_mut();
                    if (store.width - bounds.size.width).abs() > px(0.5) {
                        store.width = bounds.size.width;
                        window.refresh();
                    }
                },
                |_, (), _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        let close = (self.close == StackClose::Disc && self.pinned)
            .then(|| self.close_disc(anchor, layer, cx));
        let scrim = self.own_scrim.then(|| {
            let shut = cx.weak_entity();
            stack_scrim(
                "stack-scrim",
                frame.progress,
                layer,
                window,
                move |_, cx| {
                    let _ = shut.update(cx, Self::close);
                },
                cx,
            )
        });
        let keys = self.is_open().then(|| {
            let menu = cx.weak_entity();
            canvas(
                |_, _, _| {},
                move |_, (), window, _| {
                    let menu = menu.clone();
                    window.on_key_event(move |_: &KeyDownEvent, phase, _, cx| {
                        if phase.bubble() {
                            let _ = menu.update(cx, Self::close);
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });
        div()
            .absolute()
            .inset_0()
            .child(track_layer(Rc::clone(&self.measured)))
            .children(scrim)
            .child(column)
            .children(close)
            .children(keys)
            .into_any_element()
    }
}
