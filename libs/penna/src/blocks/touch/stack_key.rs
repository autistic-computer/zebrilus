use std::{cell::Cell, rc::Rc};

use ala::{
    AnchoredPositionMode, App, AppContext as _, Bounds, Context, ElementId, Entity, IntoElement,
    Keystroke, ParentElement as _, Pixels, Point, Render, ScrollHandle, SharedString, Styled as _,
    Task, Window, anchored, canvas, deferred, div, point, prelude::*, px,
};

use super::{
    key_press::{Grip, Press, press_listeners},
    key_row::{KEY_SIZE, KeyLook, key_glyph, key_look, key_surface, keystroke_glyph},
    stack::{STACK_HOLD, StackClose, StackGroup, StackMenu},
    stack_gesture::{STACK_SLOP, StackSlide, StackTouch},
    sticky::{StickyModifier, StickyModifiers},
};
use crate::{ActiveTheme as _, Emphasis, IconName, PaintState, Tone, rems_from_px};

const HINT: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackKeyTap {
    Latch(StickyModifier),
    Open,
}

#[derive(Clone, Copy, Debug)]
struct Touch {
    origin: Point<Pixels>,
    at: Point<Pixels>,
    grip: Grip,
    decided: bool,
    pan: Option<(Pixels, Pixels)>,
}

pub struct StackKey {
    label: SharedString,
    icon: Option<IconName>,
    keystroke: Option<Keystroke>,
    tap: StackKeyTap,
    armed: bool,
    menu: Entity<StackMenu>,
    touch: Option<Touch>,
    hold: Option<Task<()>>,
    scroll: Option<ScrollHandle>,
    face: Rc<Cell<Bounds<Pixels>>>,
}

impl StackKey {
    pub fn new(label: impl Into<SharedString>, tap: StackKeyTap, cx: &mut Context<Self>) -> Self {
        let menu = cx.new(|cx| StackMenu::new(StackClose::Anchor, cx));
        cx.observe(&menu, |_, _, cx| cx.notify()).detach();
        Self {
            label: label.into(),
            icon: None,
            keystroke: None,
            tap,
            armed: false,
            menu,
            touch: None,
            hold: None,
            scroll: None,
            face: Rc::default(),
        }
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn scrolls(mut self, scroll: ScrollHandle) -> Self {
        self.scroll = Some(scroll);
        self
    }

    #[must_use]
    pub fn groups(self, groups: Vec<StackGroup>, cx: &mut Context<Self>) -> Self {
        self.menu.update(cx, |menu, cx| menu.set_groups(groups, cx));
        self
    }

    pub fn menu(&self) -> &Entity<StackMenu> {
        &self.menu
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.menu.read(cx).is_open()
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        self.face.get()
    }

    pub fn set_keystroke(&mut self, keystroke: Option<Keystroke>, cx: &mut Context<Self>) {
        if self.keystroke != keystroke {
            self.keystroke = keystroke;
            cx.notify();
        }
    }

    pub fn set_armed(&mut self, armed: bool, cx: &mut Context<Self>) {
        if self.armed != armed {
            self.armed = armed;
            cx.notify();
        }
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.touch = None;
        self.hold = None;
        self.menu.update(cx, StackMenu::close);
        cx.notify();
    }

    fn has_groups(&self, cx: &App) -> bool {
        self.menu.read(cx).has_groups()
    }

    fn face(&self, look: KeyLook, cx: &mut Context<Self>) -> ala::Stateful<ala::Div> {
        let theme = cx.theme();
        let (surface, state) = key_look(look, theme);
        let hint = theme
            .paint_on(
                theme.layer(surface, state).ground,
                Tone::Neutral,
                Emphasis::Ghost,
            )
            .faint_at(PaintState::Rest);
        let selector = format!("stack-key-{}", self.label);
        let has_groups = self.has_groups(cx);
        key_surface(div().id(ElementId::Name(self.label.clone())), look, cx)
            .debug_selector(move || selector)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(rems_from_px(KEY_SIZE))
            .whitespace_nowrap()
            .child(match &self.keystroke {
                Some(keystroke) => keystroke_glyph(keystroke),
                None => key_glyph(self.label.clone(), self.icon.clone()),
            })
            .when(has_groups, |key| {
                key.child(
                    div()
                        .absolute()
                        .top(rems_from_px(HINT))
                        .right(rems_from_px(HINT))
                        .size(rems_from_px(HINT))
                        .rounded_full()
                        .bg(hint),
                )
            })
    }
}

impl Press for StackKey {
    fn grip(&self) -> Option<Grip> {
        self.touch.map(|touch| touch.grip)
    }

    fn grab(&mut self, at: Point<Pixels>, grip: Grip, cx: &mut Context<Self>) {
        self.touch = Some(Touch {
            origin: at,
            at,
            grip,
            decided: false,
            pan: None,
        });
        let open = self.is_open(cx);
        self.menu.update(cx, |menu, cx| {
            menu.press(at, cx);
        });
        self.hold = (!open && self.has_groups(cx)).then(|| {
            cx.spawn(async move |key, cx| {
                cx.background_executor().timer(STACK_HOLD).await;
                let _ = key.update(cx, |key, cx| {
                    key.menu.update(cx, |menu, cx| {
                        menu.hold(cx);
                    });
                });
            })
        });
        cx.notify();
    }

    fn track(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(touch) = self.touch.as_mut() else {
            return;
        };
        touch.at = at;
        if let (Some((from, offset)), Some(scroll)) = (touch.pan, &self.scroll) {
            let x = (offset + at.x - from).clamp(-scroll.max_offset().x, px(0.0));
            scroll.set_offset(point(x, scroll.offset().y));
            return;
        }
        let moved = at - touch.origin;
        if !touch.decided && moved.magnitude() >= f64::from(STACK_SLOP) {
            touch.decided = true;
            if !self.menu.read(cx).is_open()
                && moved.x.abs() > moved.y.abs()
                && let Some(scroll) = &self.scroll
            {
                touch.pan = Some((at.x, scroll.offset().x));
                self.hold = None;
                self.menu.update(cx, StackMenu::pass);
                cx.notify();
                return;
            }
        }
        let slide = if self.has_groups(cx) {
            StackSlide::UpOpens
        } else {
            StackSlide::Pass
        };
        let touch = self.menu.update(cx, |menu, cx| menu.track(at, slide, cx));
        if touch != StackTouch::Idle {
            self.hold = None;
            cx.notify();
        }
    }

    fn release(&mut self, cancelled: bool, cx: &mut Context<Self>) {
        let Some(touch) = self.touch.take() else {
            return;
        };
        self.hold = None;
        if cancelled {
            self.menu.update(cx, StackMenu::cancel);
        } else {
            let result = self.menu.update(cx, |menu, cx| menu.release(touch.at, cx));
            if result == StackTouch::Tap && touch.pan.is_none() {
                match self.tap {
                    StackKeyTap::Latch(modifier) => StickyModifiers::toggle(modifier, cx),
                    StackKeyTap::Open if self.has_groups(cx) => {
                        self.menu.update(cx, StackMenu::open);
                    }
                    StackKeyTap::Open => {}
                }
            }
        }
        cx.notify();
    }
}

impl Render for StackKey {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let latched = match self.tap {
            StackKeyTap::Latch(modifier) => StickyModifiers::get(cx).is_latched(modifier),
            StackKeyTap::Open => false,
        };
        let shown = self.menu.read(cx).is_shown();
        let pressed = self.touch.is_some_and(|touch| touch.pan.is_none());
        let look = if pressed || self.is_open(cx) || self.armed {
            KeyLook::Inverted
        } else if latched {
            KeyLook::Latched
        } else {
            KeyLook::Rest
        };
        let face = Rc::clone(&self.face);
        let anchor = self.menu.read(cx).track_anchor();
        let key = self
            .face(look, cx)
            .child(
                canvas(move |bounds, _, _| face.set(bounds), |_, (), _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .child(anchor)
            .when(!shown, |key| {
                key.child(press_listeners(cx.entity().downgrade()))
            });
        let overlay = shown.then(|| {
            let bounds = self.face.get();
            let copy = div()
                .absolute()
                .left(bounds.left())
                .top(bounds.top())
                .child(
                    self.face(look, cx)
                        .child(press_listeners(cx.entity().downgrade())),
                );
            deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Window)
                    .position(Point::default())
                    .child(
                        div()
                            .w(window.viewport_size().width)
                            .h(window.viewport_size().height)
                            .child(self.menu.clone())
                            .child(copy),
                    ),
            )
            .with_priority(3)
            .into_any_element()
        });
        key.children(overlay)
    }
}
