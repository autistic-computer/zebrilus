use ala::{
    AnyWindowHandle, App, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyBinding, Keystroke, MouseButton, ParentElement as _,
    Render, RenderOnce, Role, SharedString, StatefulInteractiveElement, StyleRefinement, Styled,
    Subscription, Window, actions, div, prelude::FluentBuilder as _,
};

use crate::{
    Disableable, Ground, Kind, PaintState, Sizable, Size, StyledExt as _, Surface, Tone,
    components::input::{field_chrome_on, field_paint_on, field_text},
    components::kbd::Kbd,
};

const CONTEXT: &str = "PennaKeyField";
const PROMPT: &str = "Press a key";
const PLACEHOLDER: &str = "None";

actions!(penna_key_field, [Record]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Record, Some(CONTEXT)),
        KeyBinding::new("space", Record, Some(CONTEXT)),
    ]);
}

pub enum KeyFieldEvent {
    Record(Keystroke),
}

pub struct KeyFieldState {
    focus_handle: FocusHandle,
    value: Option<Keystroke>,
    recording: Option<Subscription>,
    _blur: Subscription,
}

impl KeyFieldState {
    #[must_use]
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur = cx.on_blur(&focus_handle, window, |this, _, cx| this.cancel(cx));
        Self {
            focus_handle,
            value: None,
            recording: None,
            _blur: blur,
        }
    }

    #[must_use]
    pub fn value(mut self, value: Option<Keystroke>) -> Self {
        self.value = value.map(recorded);
        self
    }

    #[must_use]
    pub fn keystroke(&self) -> Option<&Keystroke> {
        self.value.as_ref()
    }

    pub fn set_value(&mut self, value: Option<Keystroke>, cx: &mut Context<Self>) {
        let value = value.map(recorded);
        if self.value != value {
            self.value = value;
            cx.notify();
        }
    }

    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    pub fn record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        if self.recording.is_some() {
            return;
        }
        let state = cx.entity().downgrade();
        let home: AnyWindowHandle = window.window_handle();
        self.recording = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != home {
                return;
            }
            let Some(state) = state.upgrade() else {
                return;
            };
            if !state.read(cx).is_recording() {
                return;
            }
            if !state.read(cx).focus_handle.is_focused(window) {
                state.update(cx, KeyFieldState::cancel);
                return;
            }
            cx.stop_propagation();
            let keystroke = &event.keystroke;
            if event.is_held || is_modifier(&keystroke.key) {
                return;
            }
            let cancel =
                keystroke.key == "escape" && keystroke.modifiers.number_of_modifiers() == 0;
            let keystroke = keystroke.clone();
            state.update(cx, |state, cx| {
                if cancel {
                    state.cancel(cx);
                } else {
                    state.commit(keystroke, cx);
                }
            });
        }));
        cx.notify();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.recording.take().is_some() {
            cx.notify();
        }
    }

    fn commit(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) {
        let keystroke = recorded(keystroke);
        self.recording = None;
        self.value = Some(keystroke.clone());
        cx.emit(KeyFieldEvent::Record(keystroke));
        cx.notify();
    }

    fn on_record(&mut self, _: &Record, window: &mut Window, cx: &mut Context<Self>) {
        self.record(window, cx);
    }
}

fn is_modifier(key: &str) -> bool {
    matches!(
        key,
        "shift" | "control" | "ctrl" | "alt" | "platform" | "cmd" | "function" | "fn"
    )
}

fn recorded(mut keystroke: Keystroke) -> Keystroke {
    let modifiers = keystroke.modifiers;
    let symbol = keystroke
        .key_char
        .as_deref()
        .filter(|text| {
            let mut chars = text.chars();
            matches!(
                (chars.next(), chars.next()),
                (Some(c), None) if !c.is_alphanumeric() && !c.is_whitespace()
            )
        })
        .map(str::to_owned);
    if modifiers.shift
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function
        && let Some(symbol) = symbol
        && symbol != keystroke.key
    {
        keystroke.modifiers.shift = false;
        keystroke.key = symbol;
    }
    keystroke
}

impl Focusable for KeyFieldState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<KeyFieldEvent> for KeyFieldState {}

impl Render for KeyFieldState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[derive(IntoElement)]
pub struct KeyField {
    state: Entity<KeyFieldState>,
    style: StyleRefinement,
    size: Size,
    disabled: bool,
    placeholder: Option<SharedString>,
    tone: Tone,
    on: Option<Ground>,
    author: Option<SharedString>,
}

impl KeyField {
    #[must_use]
    pub fn new(state: &Entity<KeyFieldState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
            size: Size::default(),
            disabled: false,
            placeholder: None,
            tone: Tone::Neutral,
            on: None,
            author: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.author = Some(id.into());
        self
    }

    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

impl Sizable for KeyField {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for KeyField {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for KeyField {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for KeyField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled;
        let (focus_handle, value, recording) = {
            let state = self.state.read(cx);
            (
                state.focus_handle.clone(),
                state.value.clone(),
                state.is_recording(),
            )
        };
        if disabled && recording {
            self.state.update(cx, KeyFieldState::cancel);
        }
        let focused = !disabled && focus_handle.is_focused(window);
        let recording = recording && focused;
        let paint_state = match (disabled, focused) {
            (true, _) => PaintState::Disabled,
            (false, true) => PaintState::Focused,
            (false, false) => PaintState::Rest,
        };
        let surface = Surface::new(Kind::Field.role())
            .tone(self.tone)
            .on_maybe(self.on);
        let paint = field_paint_on(surface, cx);
        let size = self.size;
        let cap_size = match size.rung() {
            Size::XSmall | Size::Small => Size::Small,
            _ => Size::Medium,
        };
        let spoken: SharedString = value.as_ref().map_or_else(
            || SharedString::new_static(PLACEHOLDER),
            |key| Kbd::format(key).into(),
        );
        let content = if recording {
            div()
                .min_w_0()
                .truncate()
                .text_color(paint.faint_at(paint_state))
                .child(PROMPT)
                .into_any_element()
        } else if let Some(key) = value {
            Kbd::new(key)
                .with_size(cap_size)
                .when_some(self.on, Kbd::on)
                .into_any_element()
        } else {
            div()
                .min_w_0()
                .truncate()
                .text_color(paint.faint_at(paint_state))
                .child(
                    self.placeholder
                        .unwrap_or_else(|| SharedString::new_static(PLACEHOLDER)),
                )
                .into_any_element()
        };
        let state = self.state.clone();
        let element = field_text(
            div()
                .id(ElementId::from(("penna-key-field", self.state.entity_id())))
                .role(Role::Button)
                .aria_label(spoken)
                .aria_disabled(disabled)
                .when_some(self.author, StatefulInteractiveElement::accessibility_id)
                .key_context(CONTEXT)
                .when(!disabled, |this| {
                    this.track_focus(&focus_handle.tab_stop(true))
                        .on_action(window.listener_for(&self.state, KeyFieldState::on_record))
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            state.update(cx, |state, cx| state.record(window, cx));
                        })
                })
                .flex()
                .flex_none()
                .items_center()
                .when(!recording, ala::Styled::justify_center)
                .w_full()
                .min_w_0()
                .overflow_hidden()
                .h(size.control_h())
                .px(size.input_px())
                .text_color(paint.text_at(paint_state)),
            size,
            cx,
        );
        field_chrome_on(element, surface, paint_state, true, true, cx)
            .refine_style(&self.style)
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use ala::{
        AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Keystroke,
        ParentElement as _, Render, Styled as _, TestAppContext, Window, actions, div, px,
    };

    use super::{KeyField, KeyFieldEvent, KeyFieldState};

    actions!(key_field_test, [Close]);

    struct Preview {
        state: Entity<KeyFieldState>,
        closed: Rc<Cell<usize>>,
        focus: ala::FocusHandle,
    }

    impl Render for Preview {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let closed = self.closed.clone();
            div()
                .key_context("Preview")
                .track_focus(&self.focus)
                .on_action(move |_: &Close, _, _| closed.set(closed.get() + 1))
                .size_full()
                .child(
                    div()
                        .w(px(120.0))
                        .debug_selector(|| "field".into())
                        .child(KeyField::new(&self.state)),
                )
        }
    }

    #[ala::test]
    fn recording_takes_the_next_key_before_any_binding_and_escape_cancels(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            cx.bind_keys([ala::KeyBinding::new("cmd-w", Close, Some("Preview"))]);
        });
        let closed = Rc::new(Cell::new(0));
        let shared = closed.clone();
        let (preview, cx) = cx.add_window_view(move |window, cx| Preview {
            state: cx
                .new(|cx| KeyFieldState::new(window, cx).value(Keystroke::parse("ctrl-b").ok())),
            closed: shared,
            focus: cx.focus_handle(),
        });
        let state = preview.read_with(cx, |preview, _| preview.state.clone());
        let recorded = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = recorded.clone();
        let _subscription = cx.update(|window, cx| {
            window.subscribe(&state, cx, move |_, event: &KeyFieldEvent, _, _| {
                let KeyFieldEvent::Record(key) = event;
                sink.borrow_mut().push(key.unparse());
            })
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let focus = preview.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        let field = cx.debug_bounds("field").expect("the field draws");

        cx.simulate_keystrokes("cmd-w");
        assert_eq!(closed.get(), 1);

        cx.simulate_click(field.center(), ala::Modifiers::default());
        assert!(state.read_with(cx, |state, _| state.is_recording()));
        cx.simulate_keystrokes("cmd-w");
        assert_eq!(closed.get(), 1);
        assert_eq!(*recorded.borrow(), ["cmd-w"]);
        assert!(state.read_with(cx, |state, _| !state.is_recording()));

        cx.simulate_keystrokes("enter");
        assert!(state.read_with(cx, |state, _| state.is_recording()));
        cx.simulate_keystrokes("escape");
        assert!(state.read_with(cx, |state, _| !state.is_recording()));
        assert_eq!(
            state.read_with(cx, |state, _| state.keystroke().map(Keystroke::unparse)),
            Some("cmd-w".into())
        );

        cx.simulate_keystrokes("space");
        cx.simulate_keystrokes("shift-'->\"");
        assert_eq!(*recorded.borrow(), ["cmd-w", "\""]);

        cx.simulate_keystrokes("enter");
        cx.update(|window, cx| {
            let focus = preview.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        cx.simulate_keystrokes("cmd-w");
        assert_eq!(closed.get(), 2);
        assert_eq!(recorded.borrow().len(), 2);
        assert!(state.read_with(cx, |state, _| !state.is_recording()));
    }
}
