mod checkbox;
mod radio;

use ala::{
    App, Div, InteractiveElement as _, IntoElement, MouseDownEvent, ParentElement as _, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};

pub use checkbox::Checkbox;
pub use radio::RadioGroup;

use crate::{
    ActiveTheme as _, Colorize as _, Disableable, Emphasis, Ground, Icon, IconName, PaintExt as _,
    PaintState, Sizable, Size, Smoothing, StyledExt as _, Tone, blocks::touch::TOUCH_TARGET,
    components::icon::cap_center_drop, foundation::focus::with_focus_ring, h_flex,
};

const GROUP: &str = "penna-check";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Checkbox,
    Radio,
}

#[derive(IntoElement)]
pub struct Check {
    kind: Kind,
    checked: bool,
    mixed: bool,
    tone: Option<Tone>,
    on: Option<Ground>,
    disabled: bool,
    size: Size,
    live: bool,
    focused: bool,
}

impl Check {
    #[must_use]
    pub fn checkbox(checked: bool) -> Self {
        Self::new(Kind::Checkbox, checked)
    }

    #[must_use]
    pub fn radio(checked: bool) -> Self {
        Self::new(Kind::Radio, checked)
    }

    fn new(kind: Kind, checked: bool) -> Self {
        Self {
            kind,
            checked,
            mixed: false,
            tone: None,
            on: None,
            disabled: false,
            size: Size::default(),
            live: false,
            focused: false,
        }
    }

    #[must_use]
    pub fn indeterminate(mut self, mixed: bool) -> Self {
        self.mixed = mixed;
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }
}

impl Disableable for Check {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Check {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

fn side(size: Size) -> f32 {
    match size {
        Size::Size(size) => f32::from(size),
        size => match size.rung() {
            Size::XSmall | Size::Small => 14.0,
            _ => 16.0,
        },
    }
}

fn rung(size: Size) -> Size {
    match size.rung() {
        rung @ (Size::XSmall | Size::Small) => rung,
        _ => Size::Medium,
    }
}

fn touch_slop(size: Size, window: &Window) -> (f32, f32) {
    let line = rung(size)
        .control_text()
        .line_height()
        .to_pixels(window.rem_size());
    (
        (TOUCH_TARGET - side(size)) / 2.0,
        ((TOUCH_TARGET - f32::from(line)) / 2.0).max(0.0),
    )
}

fn hold(_: &MouseDownEvent, window: &mut Window, cx: &mut App) {
    cx.stop_propagation();
    window.prevent_default();
}

fn labeled(mark: Check, label: Option<SharedString>, window: &Window, cx: &App) -> Div {
    let rung = rung(mark.size);
    let text = rung.control_text();
    let font = text.size().to_pixels(window.rem_size());
    let drop = cap_center_drop(font, px(side(mark.size)), cx);
    let faint = mark.disabled.then(|| cx.theme().foreground.faint());
    h_flex()
        .items_start()
        .gap(rung.icon_gap())
        .type_style(text, cx)
        .when_some(faint, ala::Styled::text_color)
        .child(
            h_flex()
                .flex_none()
                .h(text.line_height())
                .child(div().relative().top(drop).child(mark)),
        )
        .when_some(label, |this, label| {
            this.child(div().min_w_0().child(label))
        })
}

impl RenderOnce for Check {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let side = side(self.size);
        let theme = cx.theme();
        let ground = self.on.unwrap_or_else(|| theme.ground());
        let filled = self.checked || (self.mixed && self.kind == Kind::Checkbox);
        let paint = if filled {
            theme.paint_on(ground, self.tone.unwrap_or(theme.detail), Emphasis::Solid)
        } else {
            theme.paint_on(
                ground,
                self.tone.unwrap_or(Tone::Neutral),
                Emphasis::Outline,
            )
        };
        let ring = theme.paint_on(ground, self.tone.unwrap_or(Tone::Neutral), Emphasis::Ghost);
        let state = if self.disabled {
            PaintState::Disabled
        } else {
            PaintState::Rest
        };
        let ink = paint.text_at(state);
        let smoothing = match self.kind {
            Kind::Checkbox => theme.smoothing.exponent(),
            Kind::Radio => Smoothing::CIRCLE.exponent(),
        };
        let corner: Pixels = (theme.radius / 4.0).min(px(side / 4.0));
        let icon = |name: IconName| {
            Icon::new(name)
                .size(px((side * 0.7).round()))
                .text_color(ink)
                .into_any_element()
        };
        let mark = match self.kind {
            Kind::Checkbox if self.mixed => Some(icon(IconName::Minus)),
            Kind::Checkbox if self.checked => Some(icon(IconName::Check)),
            Kind::Radio if self.checked => Some(
                div()
                    .size(px((side * 0.375).round()))
                    .rounded_full()
                    .bg(ink)
                    .into_any_element(),
            ),
            _ => None,
        };
        let mark = h_flex()
            .flex_none()
            .justify_center()
            .size(px(side))
            .map(|mark| match self.kind {
                Kind::Checkbox => mark.rounded(corner),
                Kind::Radio => mark.rounded_full(),
            })
            .painted(paint, state)
            .children(mark);
        if self.live && !self.disabled {
            let mark = mark
                .id("box")
                .group_hover(GROUP, move |style| style.bg(paint.hover))
                .group_active(GROUP, move |style| style.bg(paint.active));
            with_focus_ring(mark, self.focused, px(0.), &ring, smoothing, window).into_any_element()
        } else {
            with_focus_ring(mark, self.focused, px(0.), &ring, smoothing, window).into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use ala::{
        Context, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, Render, TestAppContext,
        VisualTestContext, point,
    };

    use super::*;

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn press(cx: &mut VisualTestContext, keys: &[&str]) {
        for key in keys {
            let keystroke = Keystroke::parse(key).expect("valid keystroke");
            cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(KeyUpEvent { keystroke });
            draw(cx);
        }
    }

    struct Restore(Rc<RefCell<Vec<bool>>>);

    impl Render for Restore {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let flips = Rc::clone(&self.0);
            let checked = flips.borrow().last().copied();
            div().size_full().child(
                Checkbox::new("restore")
                    .label("Restore sessions")
                    .checked(checked.unwrap_or(false))
                    .indeterminate(checked.is_none())
                    .on_click(move |on, _, _| flips.borrow_mut().push(*on)),
            )
        }
    }

    #[ala::test]
    fn a_mixed_checkbox_turns_on_and_flips_on_space_or_enter_but_a_click_never_focuses_it(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let flips = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let flips = Rc::clone(&flips);
            move |_, _| Restore(flips)
        });
        draw(cx);
        cx.simulate_click(point(px(8.), px(9.)), Modifiers::default());
        draw(cx);
        assert_eq!(*flips.borrow(), [true]);
        assert!(cx.update(|window, cx| window.focused(cx).is_none()));
        cx.update(Window::focus_next);
        draw(cx);
        press(cx, &["space", "enter", "a"]);
        assert_eq!(*flips.borrow(), [true, false, true]);
    }

    struct Start(Rc<RefCell<Vec<usize>>>);

    impl Render for Start {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let picks = Rc::clone(&self.0);
            let selected = picks.borrow().last().copied();
            div().size_full().child(
                RadioGroup::new("start", ["Last session", "Home", "Empty"])
                    .selected(selected)
                    .on_select(move |index, _, _| picks.borrow_mut().push(index)),
            )
        }
    }

    #[ala::test]
    fn a_radio_group_picks_with_arrows_home_end_and_space_and_a_click_never_focuses_it(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let picks = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let picks = Rc::clone(&picks);
            move |_, _| Start(picks)
        });
        draw(cx);
        cx.simulate_click(point(px(8.), px(35.)), Modifiers::default());
        draw(cx);
        assert_eq!(*picks.borrow(), [1]);
        assert!(cx.update(|window, cx| window.focused(cx).is_none()));
        picks.borrow_mut().clear();
        cx.update(Window::focus_next);
        draw(cx);
        press(
            cx,
            &[
                "space",
                "down",
                "end",
                "end",
                "up",
                "home",
                "left",
                "shift-down",
                "enter",
            ],
        );
        assert_eq!(*picks.borrow(), [0, 1, 2, 1, 0]);
    }
}
