//! [`Input`]: the chrome around a text field, rebuilt every frame.

use zz_gpui::{
    AnyElement, App, DefiniteLength, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, Role, StatefulInteractiveElement as _, StyleRefinement, Styled,
    TextAlign, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Disableable, Emphasis, Ground, Paint, PaintExt as _, PaintState, Sizable,
    Size, StyledExt as _, Tone,
    button::Button,
    h_flex,
    icon::{Icon, IconName},
    spinner::Spinner,
};

use super::{actions, state::InputState};
use crate::Colorize as _;

const LINE_HEIGHT: DefiniteLength = DefiniteLength::Fraction(1.25);
/// A semantic hint about what a field holds, for the accessibility role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputContentType {
    Url,
    EmailAddress,
    /// A password: the rendered text is masked, single-line only. Copy and cut
    /// do nothing, and word motions treat the value as one word.
    Password,
}

impl InputContentType {
    const fn role(self) -> Role {
        match self {
            Self::Url => Role::UrlInput,
            Self::EmailAddress => Role::EmailInput,
            Self::Password => Role::PasswordInput,
        }
    }

    const fn masks(self) -> bool {
        matches!(self, Self::Password)
    }
}

/// A text field bound to an [`InputState`].
#[derive(IntoElement)]
pub struct Input {
    state: Entity<InputState>,
    style: StyleRefinement,
    size: Size,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    appearance: bool,
    bordered: bool,
    focus_bordered: bool,
    cleanable: bool,
    disabled: bool,
    tab_index: isize,
    content_type: Option<InputContentType>,
    align: TextAlign,
    tone: Tone,
    emphasis: Option<Emphasis>,
    on: Option<Ground>,
}

/// The paint of a field: `tone` at `emphasis`, or at the look's field
/// emphasis, on `on` or the window.
pub(crate) fn field_paint(
    tone: Tone,
    emphasis: Option<Emphasis>,
    on: Option<Ground>,
    cx: &App,
) -> Paint {
    let theme = cx.theme();
    theme.paint_on(
        on.unwrap_or_else(|| theme.ground()),
        tone,
        emphasis.unwrap_or_else(|| theme.field_emphasis()),
    )
}

/// A field's fill, border and text in `state`, with the hover fill while it
/// rests. The border is always at least a pixel wide, clear when the paint
/// has none, so focusing a field never moves its text.
pub(crate) fn field_chrome<E: Styled + InteractiveElement>(
    element: E,
    paint: Paint,
    state: PaintState,
    bordered: bool,
    focus_bordered: bool,
) -> E {
    let border = match state {
        PaintState::Focused if !focus_bordered => paint.border_at(PaintState::Rest),
        _ => paint.border_at(state),
    };
    let element = element.painted(paint, state);
    let element = if bordered {
        element
            .border(paint.border_width.max(px(1.0)))
            .border_color(border)
    } else {
        element.border(px(0.0))
    };
    if state == PaintState::Rest {
        element.hover(move |style| style.bg(paint.hover))
    } else {
        element
    }
}

/// The text size of a field at `size`.
pub(crate) fn field_text<E: Styled>(element: E, size: Size) -> E {
    element.text_size(size.control_text())
}

impl Input {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
            size: Size::default(),
            prefix: None,
            suffix: None,
            appearance: true,
            bordered: true,
            focus_bordered: true,
            cleanable: false,
            disabled: false,
            tab_index: 0,
            content_type: None,
            align: TextAlign::Left,
            tone: Tone::Neutral,
            emphasis: None,
            on: None,
        }
    }

    /// Draw the field in `tone`: danger for an invalid value.
    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Draw the field at `emphasis` instead of the look's field emphasis:
    /// ghost for a field that shows its edge only when hovered or focused.
    #[must_use]
    pub fn emphasis(mut self, emphasis: Emphasis) -> Self {
        self.emphasis = Some(emphasis);
        self
    }

    /// Draw the field on `ground`, the paint it sits inside, instead of the
    /// window.
    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }

    /// Place the text horizontally within the field. Hit-testing, the caret and
    /// selection quads all follow the alignment.
    #[must_use]
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    #[must_use]
    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    #[must_use]
    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }

    /// Draw the field's background, radius and border. With `false` the field
    /// is bare text, for a field embedded in chrome that already has an edge.
    #[must_use]
    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    /// Draw the resting border. Only meaningful with [`Self::appearance`].
    #[must_use]
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    /// Draw the focus ring. Only meaningful with [`Self::bordered`].
    #[must_use]
    pub fn focus_bordered(mut self, bordered: bool) -> Self {
        self.focus_bordered = bordered;
        self
    }

    /// Show a clear button while the field is non-empty. Single-line only.
    #[must_use]
    pub fn cleanable(mut self, cleanable: bool) -> Self {
        self.cleanable = cleanable;
        self
    }

    #[must_use]
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        self
    }

    /// What the field holds, for assistive technology.
    #[must_use]
    pub fn content_type(mut self, content_type: InputContentType) -> Self {
        self.content_type = Some(content_type);
        self
    }

    fn sized<E: Styled>(element: E, size: Size, density: f32) -> E {
        field_text(element.px(size.input_px()).py(size.input_py(density)), size)
    }

    fn height<E: Styled>(element: E, size: Size, density: f32) -> E {
        element.h(size.control_h(density))
    }

    fn clear_button(state: &Entity<InputState>, cx: &App) -> Button {
        let state = state.clone();
        Button::new("zz-input-clear")
            .icon(Icon::new(IconName::CircleX))
            .emphasis(Emphasis::Ghost)
            .small()
            .tab_stop(false)
            .text_color(cx.theme().foreground.muted())
            .on_click(move |_, window, cx| {
                state.update(cx, |this, cx| {
                    this.clear(cx);
                    this.focus(window, cx);
                });
            })
    }
}

impl Sizable for Input {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Input {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let paint = field_paint(self.tone, self.emphasis, self.on, cx);
        let resting = if self.disabled {
            PaintState::Disabled
        } else {
            PaintState::Rest
        };
        self.state.update(cx, |state, _| {
            state.placeholder_color = Some(paint.faint_at(resting));
            state.disabled = self.disabled;
            state.size = self.size;
            state.align = self.align;
            state.masked = self.content_type.is_some_and(InputContentType::masks);
        });

        let (multi_line, focused, empty, loading, focus_handle) = {
            let state = self.state.read(cx);
            (
                state.mode().is_multi_line(),
                state.focus_handle_ref().is_focused(window) && !self.disabled,
                state.text().is_empty(),
                state.loading,
                state.focus_handle_ref().clone(),
            )
        };

        let role = if multi_line {
            Role::MultilineTextInput
        } else {
            self.content_type
                .map_or(Role::TextInput, InputContentType::role)
        };

        let gap = match self.size {
            Size::Small => px(4.),
            Size::Medium | Size::Size(_) => px(6.),
        };
        let paint_state = if focused {
            PaintState::Focused
        } else {
            resting
        };
        let show_clear = self.cleanable && !self.disabled && !loading && !empty && !multi_line;
        let clear_button = show_clear.then(|| Self::clear_button(&self.state, cx));
        let editable = !self.disabled;
        let disabled = self.disabled;
        let spinner_color = cx.theme().foreground.muted();
        let prefix = self.prefix;
        let suffix = self.suffix;
        let has_suffix = suffix.is_some() || loading || show_clear;

        let mut element = div()
            .id(("zz-input", self.state.entity_id()))
            .role(role)
            .aria_disabled(disabled)
            .key_context(actions::CONTEXT)
            .track_focus(&focus_handle)
            .tab_index(self.tab_index)
            .flex()
            .items_center()
            .gap(gap)
            .w_full()
            .line_height(LINE_HEIGHT)
            .text_color(paint.text_at(paint_state))
            .when(editable, zz_gpui::Styled::cursor_text);

        element = Self::sized(element, self.size, cx.theme().density);
        if !multi_line {
            element = Self::height(element, self.size, cx.theme().density);
        }

        element
            .when(self.appearance, |this| {
                field_chrome(this, paint, paint_state, self.bordered, self.focus_bordered).rounded(
                    if multi_line {
                        cx.theme().radius
                    } else {
                        cx.theme().control_radius()
                    },
                )
            })
            .refine_style(&self.style)
            .when(editable, |this| {
                this.on_action(window.listener_for(&self.state, InputState::backspace))
                    .on_action(window.listener_for(&self.state, InputState::delete))
                    .on_action(
                        window.listener_for(&self.state, InputState::delete_to_previous_word_start),
                    )
                    .on_action(
                        window.listener_for(&self.state, InputState::delete_to_next_word_end),
                    )
                    .on_action(
                        window.listener_for(&self.state, InputState::delete_to_beginning_of_line),
                    )
                    .on_action(window.listener_for(&self.state, InputState::delete_to_end_of_line))
                    .on_action(window.listener_for(&self.state, InputState::enter))
                    .on_action(window.listener_for(&self.state, InputState::cut))
                    .on_action(window.listener_for(&self.state, InputState::paste))
                    .on_action(window.listener_for(&self.state, InputState::undo))
                    .on_action(window.listener_for(&self.state, InputState::redo))
                    .on_action(window.listener_for(&self.state, InputState::show_character_palette))
            })
            .on_action(window.listener_for(&self.state, InputState::move_left))
            .on_action(window.listener_for(&self.state, InputState::move_right))
            .on_action(window.listener_for(&self.state, InputState::select_left))
            .on_action(window.listener_for(&self.state, InputState::select_right))
            .on_action(window.listener_for(&self.state, InputState::move_home))
            .on_action(window.listener_for(&self.state, InputState::move_end))
            .on_action(window.listener_for(&self.state, InputState::select_to_start_of_line))
            .on_action(window.listener_for(&self.state, InputState::select_to_end_of_line))
            .on_action(window.listener_for(&self.state, InputState::move_to_start))
            .on_action(window.listener_for(&self.state, InputState::move_to_end))
            .on_action(window.listener_for(&self.state, InputState::select_to_start))
            .on_action(window.listener_for(&self.state, InputState::select_to_end))
            .on_action(window.listener_for(&self.state, InputState::move_to_previous_word))
            .on_action(window.listener_for(&self.state, InputState::move_to_next_word))
            .on_action(window.listener_for(&self.state, InputState::select_to_previous_word_start))
            .on_action(window.listener_for(&self.state, InputState::select_to_next_word_end))
            .on_action(window.listener_for(&self.state, InputState::select_all))
            .on_action(window.listener_for(&self.state, InputState::copy))
            .on_action(window.listener_for(&self.state, InputState::escape))
            .when(multi_line, |this| {
                this.on_action(window.listener_for(&self.state, InputState::move_up))
                    .on_action(window.listener_for(&self.state, InputState::move_down))
                    .on_action(window.listener_for(&self.state, InputState::select_up))
                    .on_action(window.listener_for(&self.state, InputState::select_down))
            })
            .on_key_down(window.listener_for(&self.state, InputState::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&self.state, InputState::on_mouse_down),
            )
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(&self.state, InputState::on_mouse_down),
            )
            .on_scroll_wheel(window.listener_for(&self.state, InputState::on_scroll_wheel))
            .children(prefix.map(|prefix| {
                div()
                    .flex_none()
                    .when(disabled, |this| this.opacity(0.5))
                    .child(prefix)
            }))
            .child(self.state.clone())
            .when(has_suffix, |this| {
                this.child(
                    h_flex()
                        .flex_none()
                        .gap(gap)
                        .items_center()
                        .when(disabled, |this| this.opacity(0.5))
                        .when(loading, |this| {
                            this.child(Spinner::new().small().color(spinner_color))
                        })
                        .children(clear_button)
                        .children(suffix),
                )
            })
    }
}
