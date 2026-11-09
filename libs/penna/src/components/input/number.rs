use ala::{
    App, Context, CursorStyle, ElementId, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, RenderOnce,
    Role, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window, actions,
    canvas, div, prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, Disableable, Emphasis, Ground, Kind, PaintState, Sizable, Size,
    StyledExt as _, Surface, Tone,
    components::button::Button,
    components::icon::IconName,
    foundation::{author_name, author_part},
    h_flex,
};

use super::{
    field::{Input, field_chrome_on},
    state::{InputState, StepDirection},
};
use ala::TextAlign;

actions!(penna_number_input, [Increment, Decrement]);

const CONTEXT: &str = "PennaNumberInput";
const SCRUB_STEP_PX: f32 = 4.0;
const SCRUB_SLOP_PX: f32 = 3.0;

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", Increment, Some(CONTEXT)),
        KeyBinding::new("down", Decrement, Some(CONTEXT)),
    ]);
}

pub(super) fn is_number_like(value: &str) -> bool {
    let body = value.strip_prefix(['-', '+']).unwrap_or(value);
    let mut seen_dot = false;
    for c in body.chars() {
        if c == '.' {
            if seen_dot {
                return false;
            }
            seen_dot = true;
        } else if !c.is_ascii_digit() {
            return false;
        }
    }
    true
}

fn fraction_digits(value: &str) -> usize {
    value.split('.').nth(1).map_or(0, str::len)
}

pub(super) fn stepped(
    current: &str,
    direction: StepDirection,
    step: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> Option<String> {
    let current = current.trim();
    let parsed = current.parse::<f64>().ok();
    let base = parsed.unwrap_or(0.0);

    let mut value = match direction {
        StepDirection::Increment => base + step,
        StepDirection::Decrement => base - step,
    };
    let mut digits = fraction_digits(current).max(fraction_digits(&step.to_string()));

    if let Some(min) = min
        && value < min
    {
        value = min;
        digits = digits.max(fraction_digits(&min.to_string()));
    }
    if let Some(max) = max
        && value > max
    {
        value = max;
        digits = digits.max(fraction_digits(&max.to_string()));
    }

    if let Some(base) = parsed {
        let moved = match direction {
            StepDirection::Increment => value > base,
            StepDirection::Decrement => value < base,
        };
        if !moved {
            return None;
        }
    }

    Some(format!("{value:.digits$}"))
}

pub(super) fn scrubbed(
    from: &str,
    steps: f64,
    step: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> String {
    let from = from.trim();
    let mut value = from.parse::<f64>().unwrap_or(0.0) + steps * step;
    let mut digits = fraction_digits(from).max(fraction_digits(&step.to_string()));
    if let Some(min) = min
        && value < min
    {
        value = min;
        digits = digits.max(fraction_digits(&min.to_string()));
    }
    if let Some(max) = max
        && value > max
    {
        value = max;
        digits = digits.max(fraction_digits(&max.to_string()));
    }
    format!("{value:.digits$}")
}

#[derive(Default)]
struct Scrub {
    from: Option<(MouseDownEvent, String)>,
    moved: bool,
}

impl InputState {
    fn on_increment(&mut self, _: &Increment, _: &mut Window, cx: &mut Context<Self>) {
        self.step_value(StepDirection::Increment, cx);
    }

    fn on_decrement(&mut self, _: &Decrement, _: &mut Window, cx: &mut Context<Self>) {
        self.step_value(StepDirection::Decrement, cx);
    }
}

#[derive(IntoElement)]
pub struct NumberInput {
    state: Entity<InputState>,
    style: StyleRefinement,
    size: Size,
    appearance: bool,
    disabled: bool,
    tone: Tone,
    on: Option<Ground>,
    author: Option<SharedString>,
}

impl NumberInput {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
            size: Size::default(),
            appearance: true,
            tone: Tone::Neutral,
            on: None,
            disabled: false,
            author: None,
        }
    }

    #[must_use]
    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.author = Some(author_name(&id.into()));
        self
    }

    fn stepper(
        state: &Entity<InputState>,
        id: ElementId,
        icon: IconName,
        direction: StepDirection,
        size: Size,
        disabled: bool,
        ground: Ground,
    ) -> Button {
        let state = state.clone();
        let slop = crate::blocks::touch::control_slop(size);
        Button::new(id)
            .emphasis(Emphasis::Ghost)
            .on(ground)
            .hit_slop(slop, slop)
            .with_size(size)
            .icon(icon)
            .compact()
            .h_full()
            .tab_stop(false)
            .disabled(disabled)
            .on_click(move |_, window, cx| {
                state.update(cx, |this, cx| {
                    this.focus(window, cx);
                    this.step_value(direction, cx);
                });
            })
    }
}

impl NumberInput {
    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }
}

impl Sizable for NumberInput {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for NumberInput {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for NumberInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn scrub_area(state: &Entity<InputState>, scrub: &Entity<Scrub>) -> impl IntoElement {
    let press_state = state.clone();
    let press_scrub = scrub.clone();
    let state = state.clone();
    let scrub = scrub.clone();
    div()
        .absolute()
        .inset_0()
        .cursor_ew_resize()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            let from = press_state.update(cx, |state, cx| {
                state.focus(window, cx);
                state.value().to_string()
            });
            press_scrub.update(cx, |scrub, cx| {
                scrub.from = Some((event.clone(), from));
                scrub.moved = false;
                cx.notify();
            });
        })
        .child(
            canvas(
                |_, _, _| {},
                move |_, (), window, cx| {
                    if scrub.read(cx).from.is_some() {
                        window.set_window_cursor_style(CursorStyle::ResizeLeftRight);
                    }
                    let moved_scrub = scrub.clone();
                    let moved_state = state.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        let Some((start, from)) = moved_scrub.read(cx).from.clone() else {
                            return;
                        };
                        if event.pressed_button != Some(MouseButton::Left) {
                            moved_scrub.update(cx, |scrub, cx| {
                                scrub.from = None;
                                cx.notify();
                            });
                            return;
                        }
                        let dx = (event.position.x - start.position.x).as_f32();
                        if !moved_scrub.read(cx).moved && dx.abs() < SCRUB_SLOP_PX {
                            return;
                        }
                        moved_scrub.update(cx, |scrub, _| scrub.moved = true);
                        let steps = (dx / SCRUB_STEP_PX).trunc();
                        moved_state.update(cx, |state, cx| {
                            state.scrub_to(&from, f64::from(steps), cx);
                        });
                    });
                    let released_scrub = scrub.clone();
                    let released_state = state.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if !phase.bubble() || released_scrub.read(cx).from.is_none() {
                            return;
                        }
                        let (press, moved) = released_scrub.update(cx, |scrub, cx| {
                            cx.notify();
                            (scrub.from.take(), scrub.moved)
                        });
                        if let Some((press, _)) = press.filter(|_| !moved) {
                            released_state.update(cx, |state, cx| {
                                state.on_mouse_down(&press, window, cx);
                                state.end_drag();
                            });
                        }
                    });
                },
            )
            .absolute()
            .inset_0(),
        )
}

impl RenderOnce for NumberInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.state.update(cx, InputState::mark_numeric);

        let focused = self.state.read(cx).focus_handle_ref().is_focused(window) && !self.disabled;
        let role = Kind::Field.role();
        let surface = Surface::new(role).tone(self.tone).on_maybe(self.on);
        let state = match (self.disabled, focused) {
            (true, _) => PaintState::Disabled,
            (false, true) => PaintState::Focused,
            (false, false) => PaintState::Rest,
        };
        let ground = cx.theme().layer(surface, state).ground;
        let scrub = window.use_keyed_state(
            ("penna-number-scrub", self.state.entity_id()),
            cx,
            |_, _| Scrub::default(),
        );
        let scrubs = !self.disabled
            && !crate::blocks::touch::CoarsePointer::get(cx)
            && (!focused || scrub.read(cx).from.is_some());

        h_flex()
            .id(("penna-number-input", self.state.entity_id()))
            .role(Role::SpinButton)
            .when_some(
                self.author.clone(),
                StatefulInteractiveElement::accessibility_id,
            )
            .aria_disabled(self.disabled)
            .key_context(CONTEXT)
            .on_action(window.listener_for(&self.state, InputState::on_increment))
            .on_action(window.listener_for(&self.state, InputState::on_decrement))
            .w_full()
            .h(self.size.control_h())
            .items_center()
            .when(self.appearance, |this| {
                field_chrome_on(this, surface, state, true, true, cx)
            })
            .refine_style(&self.style)
            .child(Self::stepper(
                &self.state,
                author_part(self.author.as_ref(), "decrement", "penna-number-decrement"),
                IconName::Minus,
                StepDirection::Decrement,
                self.size,
                self.disabled,
                ground,
            ))
            .child(
                div()
                    .relative()
                    .flex()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Input::new(&self.state)
                            .when_some(self.author.as_ref(), |input, author| {
                                input.id(SharedString::from(format!("{author}-field")))
                            })
                            .with_size(self.size)
                            .disabled(self.disabled)
                            .tone(self.tone)
                            .when_some(self.on, Input::on)
                            .appearance(false)
                            .text_align(TextAlign::Center)
                            .h_full()
                            .flex_1()
                            .min_w_0(),
                    )
                    .when(scrubs, |field| field.child(scrub_area(&self.state, &scrub))),
            )
            .child(Self::stepper(
                &self.state,
                author_part(self.author.as_ref(), "increment", "penna-number-increment"),
                IconName::Plus,
                StepDirection::Increment,
                self.size,
                self.disabled,
                ground,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_shapes_are_accepted_including_partial_ones() {
        for value in ["", "-", "+", ".", "1", "-1", "1.", "1.5", "0.05", "256"] {
            assert!(is_number_like(value), "{value:?} should be accepted");
        }
        for value in ["a", "1a", "1.2.3", "1 2", "--1", "1-", "1e5"] {
            assert!(!is_number_like(value), "{value:?} should be rejected");
        }
    }

    #[test]
    fn stepping_keeps_the_widest_precision() {
        assert_eq!(
            stepped("0.1", StepDirection::Increment, 0.2, None, None).as_deref(),
            Some("0.3")
        );
        assert_eq!(
            stepped("1", StepDirection::Increment, 0.05, None, None).as_deref(),
            Some("1.05")
        );
        assert_eq!(
            stepped("4", StepDirection::Decrement, 1.0, None, None).as_deref(),
            Some("3")
        );
    }

    #[test]
    fn stepping_clamps_to_the_bounds() {
        assert_eq!(
            stepped("255", StepDirection::Increment, 4.0, Some(0.0), Some(256.0)).as_deref(),
            Some("256")
        );
        assert_eq!(
            stepped("2", StepDirection::Decrement, 4.0, Some(0.0), Some(256.0)).as_deref(),
            Some("0")
        );
    }

    #[test]
    fn scrubbing_moves_whole_steps_from_where_it_started_and_clamps() {
        assert_eq!(scrubbed("0.7", 2.0, 0.1, Some(0.0), Some(1.0)), "0.9");
        assert_eq!(scrubbed("0.7", 5.0, 0.1, Some(0.0), Some(1.0)), "1.0");
        assert_eq!(scrubbed("100", -3.0, 1.0, None, None), "97");
        assert_eq!(scrubbed("2", -4.0, 1.0, Some(0.0), None), "0");
    }

    #[test]
    fn a_step_that_cannot_move_the_value_does_nothing() {
        assert_eq!(
            stepped("256", StepDirection::Increment, 1.0, None, Some(256.0)),
            None
        );
        assert_eq!(
            stepped("0", StepDirection::Decrement, 1.0, Some(0.0), None),
            None
        );
        assert_eq!(
            stepped("-5", StepDirection::Decrement, 1.0, Some(0.0), None),
            None
        );
    }

    #[test]
    fn an_empty_value_steps_into_range() {
        assert_eq!(
            stepped("", StepDirection::Increment, 1.0, Some(10.0), None).as_deref(),
            Some("10")
        );
        assert_eq!(
            stepped("", StepDirection::Increment, 1.0, None, None).as_deref(),
            Some("1")
        );
    }

    struct Field {
        state: Entity<InputState>,
    }

    impl ala::Render for Field {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            ala::div()
                .w(ala::px(160.0))
                .child(NumberInput::new(&self.state))
        }
    }

    #[ala::test]
    fn arrow_keys_step_a_focused_number_input(cx: &mut ala::TestAppContext) {
        use ala::AppContext as _;

        cx.update(crate::init);
        let (field, cx) = cx.add_window_view(|window, cx| Field {
            state: cx.new(|cx| InputState::new(window, cx).default_value("4")),
        });
        let state = field.read_with(cx, |field, _| field.state.clone());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            state.update(cx, |state, cx| state.focus(window, cx));
        });

        cx.simulate_keystrokes("up up");
        assert_eq!(state.read_with(cx, |state, _| state.value()), "6");
        cx.simulate_keystrokes("down");
        assert_eq!(state.read_with(cx, |state, _| state.value()), "5");
    }

    #[ala::test]
    fn sliding_on_the_value_scrubs_it_a_step_per_four_points(cx: &mut ala::TestAppContext) {
        use ala::{AppContext as _, Modifiers, point, px};

        cx.update(crate::init);
        let (field, cx) = cx.add_window_view(|window, cx| Field {
            state: cx.new(|cx| InputState::new(window, cx).default_value("4").min(0.0)),
        });
        let state = field.read_with(cx, |field, _| field.state.clone());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let start = point(px(80.0), px(10.0));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            point(px(92.0), px(10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert_eq!(state.read_with(cx, |state, _| state.value()), "7");
        cx.simulate_mouse_move(
            point(px(40.0), px(10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert_eq!(state.read_with(cx, |state, _| state.value()), "0");
        cx.simulate_mouse_up(
            point(px(40.0), px(10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_move(point(px(120.0), px(10.0)), None, Modifiers::none());
        assert_eq!(state.read_with(cx, |state, _| state.value()), "0");
    }
}
