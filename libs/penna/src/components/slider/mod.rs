use crate::Space;
use std::{ops::RangeInclusive, rc::Rc};

use ala::{
    AnyElement, App, Bounds, Context, Div, ElementId, Entity, FocusHandle, HitboxBehavior, Hsla,
    IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, RenderOnce, Role, SharedString,
    Stateful, TouchDragEvent, TouchPhase, Window, canvas, div, linear_color_stop, linear_gradient,
    prelude::*, px, relative,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Disableable, Emphasis, Kind, Label,
    Paint, PaintState, Sizable, Size, Smoothing, StyledExt as _, Surface, TextStyle, Tone,
    blocks::sheet::{bottom_sheet, close_sheet, floating_sheet, sheet_close, sheet_inset},
    blocks::touch::CoarsePointer,
    components::input::{field_chrome_on, field_text},
    components::tooltip::Tooltip,
    foundation::focus::with_focus_ring,
    h_flex, oklab_lightness, v_flex,
};

type ChangeHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type ValueHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;
type ReleaseHandler = Rc<dyn Fn(&mut Window, &mut App)>;
type Display = Rc<dyn Fn(f32) -> SharedString>;

const PREVIEW_WIDTH: f32 = 72.0;
const PREVIEW_TRACK: f32 = 4.0;
const PREVIEW_THUMB: f32 = 10.0;
const DIM: f32 = 0.5;

#[derive(Clone, Copy)]
struct Metrics {
    size: Size,
    rail: f32,
    knob: f32,
}

impl Metrics {
    fn new(size: Size, coarse: bool) -> Self {
        let size = if coarse { Size::Touch } else { size };
        let (rail, knob) = match size.rung() {
            Size::XSmall => (4., 12.),
            Size::Small => (4., 14.),
            Size::Large => (6., 18.),
            Size::XLarge => (6., 22.),
            Size::Touch => (6., 28.),
            _ => (6., 16.),
        };
        Self { size, rail, knob }
    }
}

fn focus_ring<T: ParentElement + Styled>(this: T, ringed: bool, window: &Window, cx: &App) -> T {
    let paint = cx.theme().paint(Tone::Neutral, Emphasis::Ghost);
    with_focus_ring(
        this,
        ringed,
        px(0.),
        &paint,
        Smoothing::CIRCLE.exponent(),
        window,
    )
}

pub(crate) fn thumb(cx: &App) -> f32 {
    Metrics::new(Size::Medium, CoarsePointer::get(cx)).knob
}

fn dim(color: Hsla, disabled: bool) -> Hsla {
    if disabled { color.opacity(DIM) } else { color }
}

#[derive(IntoElement)]
pub struct DiscreteSlider {
    id: ElementId,
    labels: Vec<SharedString>,
    selected: usize,
    label: SharedString,
    size: Size,
    disabled: bool,
    on_change: ChangeHandler,
}

impl DiscreteSlider {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        labels: Vec<SharedString>,
        selected: usize,
        on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            labels,
            selected,
            label: label.into(),
            size: Size::Medium,
            disabled: false,
            on_change: Rc::new(on_change),
        }
    }
}

impl Sizable for DiscreteSlider {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for DiscreteSlider {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

struct StepFocus {
    focus: FocusHandle,
    pointer: bool,
}

impl StepFocus {
    fn set_pointer(&mut self, pointer: bool, cx: &mut Context<Self>) {
        if self.pointer != pointer {
            self.pointer = pointer;
            cx.notify();
        }
    }
}

impl RenderOnce for DiscreteSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = self.labels.len();
        let selected = self.selected.min(count.saturating_sub(1));
        let disabled = self.disabled;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| StepFocus {
            focus: cx.focus_handle().tab_stop(true),
            pointer: false,
        });
        let focus = state.read(cx).focus.clone();
        let focused = !disabled && focus.is_focused(window);
        if !focused && state.read(cx).pointer {
            state.update(cx, |steps, _| steps.pointer = false);
        }
        let ringed = focused && !state.read(cx).pointer;
        let foreground = cx.theme().foreground;
        let (label_color, value_color) = if disabled {
            (foreground.faint(), foreground.faint())
        } else {
            (foreground.muted(), foreground)
        };
        let rail = Metrics::new(self.size, false).rail;
        let text = match self.size.rung() {
            Size::XSmall | Size::Small => TextStyle::Caption,
            _ => TextStyle::Small,
        };
        let filled = cx.theme().paint(Tone::Accent, Emphasis::Solid);
        let track = cx.theme().paint(Tone::Neutral, Emphasis::Soft);
        let value = self.labels.get(selected).cloned().unwrap_or_default();
        let mut pills = h_flex()
            .flex_1()
            .min_w_0()
            .max_w(px(
                count as f32 * 24.0 + count.saturating_sub(1) as f32 * 4.0
            ))
            .gap(Space::S4);
        for (index, label) in self.labels.iter().enumerate() {
            let change = self.on_change.clone();
            let focus = focus.clone();
            let tooltip = label.clone();
            let selector = format!("{}:pill-{index}", self.id);
            let group = SharedString::from(selector.clone());
            let (fill, hover) = if index <= selected {
                (filled.fill, filled.hover)
            } else {
                (track.active, past_active(&track))
            };
            pills = pills.child(
                h_flex()
                    .id(index)
                    .group(group.clone())
                    .debug_selector(move || selector.clone())
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .flex_1()
                    .min_w_0()
                    .h(self.size.control_h())
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .child(focus_ring(
                        div()
                            .w_full()
                            .h(px(rail))
                            .rounded_full()
                            .bg(dim(fill, disabled))
                            .when(!disabled, |bar| {
                                bar.group_hover(group, move |this| this.bg(hover))
                            }),
                        ringed && index == selected,
                        window,
                        cx,
                    ))
                    .when(!disabled, |pill| {
                        pill.click_cursor().on_click(move |_, window, cx| {
                            focus.focus(window, cx);
                            change(index, window, cx);
                        })
                    }),
            );
        }
        let pressed = state.clone();
        let on_change = self.on_change;
        h_flex()
            .author_id(self.id)
            .role(Role::Slider)
            .aria_label(self.label.clone())
            .aria_value(value.clone())
            .aria_disabled(disabled)
            .w_full()
            .gap(Space::S8)
            .type_style(text, cx)
            .when(!disabled, |this| {
                this.track_focus(&focus)
                    .on_any_mouse_down(move |_, _, cx| {
                        pressed.update(cx, |steps, cx| steps.set_pointer(true, cx));
                    })
                    .on_key_down(move |event, window, cx| {
                        let next = match event.keystroke.key.as_str() {
                            "left" | "down" => selected.saturating_sub(1),
                            "right" | "up" => (selected + 1).min(count.saturating_sub(1)),
                            "home" => 0,
                            "end" => count.saturating_sub(1),
                            _ => return,
                        };
                        cx.stop_propagation();
                        state.update(cx, |steps, cx| steps.set_pointer(false, cx));
                        if count > 0 && next != selected {
                            on_change(next, window, cx);
                        }
                    })
            })
            .child(div().flex_none().text_color(label_color).child(self.label))
            .child(pills)
            .child(
                div()
                    .flex_none()
                    .w(px(52.0))
                    .text_color(value_color)
                    .child(value),
            )
    }
}

const STOP_TRACK: f32 = 26.0;
const STOP_KNOB: f32 = 30.0;
const STOP_DOT: f32 = 4.0;
const STOP_RING: f32 = 6.0;
const STOP_HEAD: f32 = 26.0;

#[derive(Clone, Copy)]
struct Stops {
    count: usize,
    open_start: bool,
    picked: bool,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a step index: whole, never negative and far below usize"
)]
fn step_index(value: f32) -> usize {
    value.round().max(0.0) as usize
}

#[derive(IntoElement)]
pub struct StepRail {
    id: ElementId,
    label: SharedString,
    steps: Vec<SharedString>,
    picked: Option<usize>,
    open_start: bool,
    disabled: bool,
    on_change: ChangeHandler,
}

impl StepRail {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        steps: Vec<SharedString>,
        picked: Option<usize>,
        on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            steps,
            picked,
            open_start: false,
            disabled: false,
            on_change: Rc::new(on_change),
        }
    }

    #[must_use]
    pub fn open_start(mut self) -> Self {
        self.open_start = true;
        self
    }
}

impl Disableable for StepRail {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for StepRail {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a handful of steps, far below f32 precision"
    )]
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = self.steps.len();
        let picked = self.picked.filter(|picked| *picked < count);
        let drag = drag_state(&self.id, window, cx);
        let shown = drag.read(cx).live.map(step_index).or(picked);
        let name = shown
            .and_then(|at| self.steps.get(at).cloned())
            .unwrap_or_default();
        let foreground = cx.theme().foreground;
        let (label_color, value_color) = if self.disabled {
            (foreground.faint(), foreground.faint())
        } else {
            (foreground.muted(), foreground)
        };
        let names = self.steps;
        let on_change = self.on_change;
        let mut slider = Slider::new(
            self.id,
            self.label.clone(),
            picked.unwrap_or(0) as f32,
            0.0..=count.saturating_sub(1) as f32,
            move |value, window, cx| on_change(step_index(value), window, cx),
        )
        .step(1.0)
        .commit_on_release()
        .inline()
        .display(move |value| names.get(step_index(value)).cloned().unwrap_or_default())
        .disabled(self.disabled);
        slider.stops = Some(Stops {
            count,
            open_start: self.open_start,
            picked: shown.is_some(),
        });
        slider.drag = Some(drag);
        v_flex()
            .w_full()
            .child(
                h_flex()
                    .w_full()
                    .h(px(STOP_HEAD))
                    .justify_between()
                    .child(
                        Label::new(TextStyle::Caption, self.label)
                            .strong()
                            .text_color(label_color),
                    )
                    .child(Label::new(TextStyle::Small, name).text_color(value_color)),
            )
            .child(slider)
    }
}

fn past_active(paint: &Paint) -> Hsla {
    let step = (paint.active.a - paint.hover.a).max(0.0);
    paint.active.alpha((paint.active.a + step).min(1.0))
}

#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    label: SharedString,
    value: f32,
    scale: Scale,
    track: Vec<Hsla>,
    display: Option<Display>,
    inline: bool,
    commit_on_release: bool,
    focus: Option<FocusHandle>,
    stops: Option<Stops>,
    drag: Option<Entity<Drag>>,
    size: Size,
    disabled: bool,
    on_change: ValueHandler,
}

impl Slider {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: f32,
        range: RangeInclusive<f32>,
        on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value,
            scale: Scale {
                min: *range.start(),
                max: *range.end(),
                step: 0.0,
            },
            track: Vec::new(),
            display: None,
            inline: false,
            commit_on_release: false,
            focus: None,
            stops: None,
            drag: None,
            size: Size::Medium,
            disabled: false,
            on_change: Rc::new(on_change),
        }
    }

    #[must_use]
    pub fn commit_on_release(mut self) -> Self {
        self.commit_on_release = true;
        self
    }

    #[must_use]
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    fn focus(&self, drag: &Entity<Drag>, cx: &App) -> FocusHandle {
        self.focus
            .clone()
            .unwrap_or_else(|| drag.read(cx).focus.clone())
    }

    #[must_use]
    pub fn step(mut self, step: f32) -> Self {
        self.scale.step = step.max(0.0);
        self
    }

    #[must_use]
    pub fn track(mut self, colors: impl IntoIterator<Item = Hsla>) -> Self {
        self.track = colors.into_iter().collect();
        self
    }

    #[must_use]
    pub fn display(mut self, display: impl Fn(f32) -> SharedString + 'static) -> Self {
        self.display = Some(Rc::new(display));
        self
    }

    #[must_use]
    pub fn inline(mut self) -> Self {
        self.inline = true;
        self
    }

    #[must_use]
    pub fn shown(&self) -> SharedString {
        self.read(self.value)
    }

    fn read(&self, value: f32) -> SharedString {
        match &self.display {
            Some(display) => display(value),
            None => self.scale.read(value),
        }
    }
}

impl Sizable for Slider {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Slider {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Clone, Copy)]
struct Scale {
    min: f32,
    max: f32,
    step: f32,
}

impl Scale {
    fn snap(self, value: f32) -> f32 {
        let value = value.clamp(self.min, self.max.max(self.min));
        if self.step > 0.0 {
            (self.min + ((value - self.min) / self.step).round() * self.step)
                .clamp(self.min, self.max.max(self.min))
        } else {
            value
        }
    }

    fn fraction(self, value: f32) -> f32 {
        if self.max > self.min {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn at(self, x: Pixels, bounds: Bounds<Pixels>, thumb: f32) -> f32 {
        let radius = px(thumb / 2.0);
        let width = (bounds.size.width - radius * 2.0).max(px(1.0));
        let fraction = ((x - bounds.left() - radius) / width).clamp(0.0, 1.0);
        self.snap(self.min + fraction * (self.max - self.min))
    }

    fn nudge(self, value: f32, steps: f32) -> f32 {
        let step = if self.step > 0.0 {
            self.step
        } else {
            (self.max - self.min) / 100.0
        };
        self.snap(value + step * steps)
    }

    fn read(self, value: f32) -> SharedString {
        if self.step >= 1.0 {
            format!("{value:.0}")
        } else if self.step >= 0.1 {
            format!("{value:.1}")
        } else {
            format!("{value:.2}")
        }
        .into()
    }
}

struct Drag {
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    live: Option<f32>,
    touch: bool,
    sheet: bool,
    pointer: bool,
}

impl Drag {
    fn set_pointer(&mut self, pointer: bool, cx: &mut Context<Self>) {
        if self.pointer != pointer {
            self.pointer = pointer;
            cx.notify();
        }
    }

    fn set_sheet(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.sheet != open {
            self.sheet = open;
            cx.notify();
        }
    }
}

fn drag_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<Drag> {
    window.use_keyed_state(id.clone(), cx, |_, cx| Drag {
        focus: cx.focus_handle().tab_stop(true),
        bounds: Bounds::default(),
        live: None,
        touch: false,
        sheet: false,
        pointer: false,
    })
}

fn knob_fill(cx: &App, disabled: bool) -> Hsla {
    let theme = cx.theme();
    dim(
        if oklab_lightness(theme.background) >= oklab_lightness(theme.foreground) {
            theme.background
        } else {
            theme.foreground
        },
        disabled,
    )
}

impl RenderOnce for Slider {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let drag = match self.drag.take() {
            Some(drag) => drag,
            None => drag_state(&self.id, window, cx),
        };
        if drag.read(cx).pointer && !self.focus(&drag, cx).is_focused(window) {
            drag.update(cx, |drag, _| drag.pointer = false);
        }
        if CoarsePointer::get(cx) && !self.inline {
            self.field(drag, window, cx).into_any_element()
        } else {
            self.slider(drag, window, cx).into_any_element()
        }
    }
}

impl Slider {
    fn rail(
        &self,
        fraction: f32,
        track: f32,
        thumb: f32,
        ringed: bool,
        window: &Window,
        cx: &App,
    ) -> Div {
        if let Some(stops) = self.stops {
            return self.stop_rail(stops, fraction, ringed, window, cx);
        }
        let theme = cx.theme();
        let disabled = self.disabled;
        let filled = dim(theme.paint(Tone::Accent, Emphasis::Solid).fill, disabled);
        let rest = dim(
            theme
                .paint(Tone::Neutral, Emphasis::Soft)
                .fill_at(PaintState::Active),
            disabled,
        );
        let knob = knob_fill(cx, disabled);
        let bar = if self.track.len() > 1 {
            let last = self.track.len() - 2;
            div()
                .flex()
                .h(px(track))
                .w_full()
                .children(self.track.windows(2).enumerate().map(|(index, pair)| {
                    div()
                        .flex_1()
                        .h_full()
                        .when(index == 0, Styled::rounded_l_full)
                        .when(index == last, Styled::rounded_r_full)
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(dim(pair[0], disabled), 0.0),
                            linear_color_stop(dim(pair[1], disabled), 1.0),
                        ))
                }))
        } else {
            div().h(px(track)).w_full().rounded_full().bg(rest).child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(filled),
            )
        };
        let dot = focus_ring(
            div()
                .absolute()
                .top(px((track - thumb) / 2.0))
                .left(relative(fraction))
                .ml(px(-thumb / 2.0))
                .size(px(thumb))
                .rounded_full()
                .bg(knob)
                .shadow(crate::knob_shadow(cx)),
            ringed,
            window,
            cx,
        );
        div().relative().w_full().child(bar).child(dot)
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "a handful of steps, far below f32 precision"
    )]
    fn stop_rail(
        &self,
        stops: Stops,
        fraction: f32,
        ringed: bool,
        window: &Window,
        cx: &App,
    ) -> Div {
        let theme = cx.theme();
        let disabled = self.disabled;
        let filled = theme.paint(Tone::Accent, Emphasis::Solid);
        let rest = dim(
            theme
                .paint(Tone::Neutral, Emphasis::Soft)
                .fill_at(PaintState::Active),
            disabled,
        );
        let unlit = dim(theme.foreground.muted(), disabled);
        let lit = dim(filled.text.opacity(0.7), disabled);
        let last = stops.count.saturating_sub(1).max(1) as f32;
        let at = (fraction * last).round();
        let pad = (STOP_KNOB - 4.0) / 2.0;
        let bar = h_flex()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-pad))
            .right(px(-pad))
            .rounded_full()
            .bg(rest)
            .when(stops.picked && at > 0.0, |bar| {
                bar.child(
                    div()
                        .h_full()
                        .flex_basis(px(pad * 2.0))
                        .flex_grow(fraction)
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(dim(filled.fill, disabled)),
                )
                .child(div().h_full().flex_basis(px(0.0)).flex_grow(1.0 - fraction))
            });
        let dots = (0..stops.count).map(|index| {
            let ink = if stops.picked && (index as f32) < at {
                lit
            } else {
                unlit
            };
            let ring = stops.open_start && index == 0;
            let size = if ring { STOP_RING } else { STOP_DOT };
            div()
                .absolute()
                .top(px((STOP_TRACK - size) / 2.0))
                .left(relative(index as f32 / last))
                .ml(px(-size / 2.0))
                .size(px(size))
                .rounded_full()
                .map(|dot| {
                    if ring {
                        dot.border_1().border_color(ink)
                    } else {
                        dot.bg(ink)
                    }
                })
        });
        let knob = stops.picked.then(|| {
            focus_ring(
                div()
                    .absolute()
                    .top(px((STOP_TRACK - STOP_KNOB) / 2.0))
                    .left(relative(fraction))
                    .ml(px(-STOP_KNOB / 2.0))
                    .size(px(STOP_KNOB))
                    .rounded_full()
                    .bg(knob_fill(cx, disabled))
                    .shadow(crate::knob_shadow(cx)),
                ringed,
                window,
                cx,
            )
        });
        div()
            .relative()
            .w_full()
            .h(px(STOP_TRACK))
            .child(bar)
            .children(dots)
            .children(knob)
    }

    fn field(self, drag: Entity<Drag>, window: &Window, cx: &App) -> Div {
        let value = self.scale.snap(drag.read(cx).live.unwrap_or(self.value));
        let fraction = self.scale.fraction(value);
        let shown = self.read(value);
        let open = drag.read(cx).sheet && !self.disabled;
        let size = self.size;
        let state = if self.disabled {
            PaintState::Disabled
        } else if open {
            PaintState::Focused
        } else {
            PaintState::Rest
        };
        let opener = drag.clone();
        let opener_focus = self.focus(&drag, cx);
        let trigger = field_chrome_on(
            field_text(
                h_flex()
                    .author_id(self.id.clone())
                    .role(Role::Slider)
                    .aria_label(self.label.clone())
                    .aria_value(shown.clone())
                    .aria_disabled(self.disabled)
                    .w_full()
                    .h(size.control_h())
                    .px(size.input_px())
                    .gap(Space::S10)
                    .justify_between()
                    .when(!self.disabled, crate::StyledExt::click_cursor),
                size,
                cx,
            ),
            Surface::new(Kind::Field.role()),
            state,
            true,
            true,
            cx,
        )
        .child(div().min_w_0().truncate().child(shown.clone()))
        .child(
            div()
                .flex_none()
                .w(px(PREVIEW_WIDTH))
                .px(px(PREVIEW_THUMB / 2.0))
                .child(self.rail(fraction, PREVIEW_TRACK, PREVIEW_THUMB, false, window, cx)),
        )
        .when(!self.disabled, |trigger| {
            trigger.on_click(move |_, window, cx| {
                opener_focus.focus(window, cx);
                opener.update(cx, |drag, cx| drag.set_sheet(true, cx));
            })
        });
        let sheet = open.then(|| self.sheet(drag, shown, window, cx));
        div().w_full().child(trigger).children(sheet)
    }

    fn sheet(
        self,
        drag: Entity<Drag>,
        shown: SharedString,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let id = drag.entity_id();
        let dismiss = drag.clone();
        floating_sheet(
            bottom_sheet(
                ("penna-slider-sheet", id),
                self.label.clone(),
                [sheet_close(("penna-slider-sheet-close", id))
                    .on_click(move |_, _, cx| close_sheet(("penna-slider-sheet", id), cx))
                    .into_any_element()],
                v_flex()
                    .px(Space::INSET_PANE)
                    .pb(Space::S16)
                    .gap(Space::S8)
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .type_style(TextStyle::Title, cx)
                            .text_color(cx.theme().foreground)
                            .child(shown),
                    )
                    .child(self.slider(drag, window, cx)),
                sheet_inset(window),
                move |_, cx| dismiss.update(cx, |drag, cx| drag.set_sheet(false, cx)),
            ),
            window,
        )
    }

    fn slider(self, drag: Entity<Drag>, window: &Window, cx: &App) -> Stateful<Div> {
        let scale = self.scale;
        let fat = self.stops.is_some();
        let metrics = if fat {
            Metrics {
                size: self.size,
                rail: STOP_TRACK,
                knob: STOP_KNOB,
            }
        } else {
            Metrics::new(self.size, CoarsePointer::get(cx))
        };
        let thumb = metrics.knob;
        let disabled = self.disabled;
        let focus = self.focus(&drag, cx);
        let ringed = !disabled && focus.is_focused(window) && !drag.read(cx).pointer;
        let value = scale.snap(drag.read(cx).live.unwrap_or(self.value));
        let rail = self.rail(
            scale.fraction(value),
            metrics.rail,
            thumb,
            ringed,
            window,
            cx,
        );
        let spoken = if fat {
            self.read(value)
        } else {
            SharedString::from(format!("{value}"))
        };
        let on_change = self.on_change;
        let deferred = self.commit_on_release;
        let committed = scale.snap(self.value);
        let emit: ValueHandler = Rc::new({
            let drag = drag.clone();
            let on_change = Rc::clone(&on_change);
            move |next, window, cx| {
                let previous = drag.update(cx, |drag, cx| {
                    cx.notify();
                    drag.live.replace(next)
                });
                if !deferred && previous != Some(next) {
                    on_change(next, window, cx);
                }
            }
        });
        let release: ReleaseHandler = Rc::new({
            let drag = drag.clone();
            move |window, cx| {
                let live = drag.update(cx, |drag, cx| {
                    drag.touch = false;
                    cx.notify();
                    drag.live.take()
                });
                if deferred
                    && let Some(live) = live
                    && live != committed
                {
                    on_change(live, window, cx);
                }
            }
        });
        let listeners = canvas(
            {
                let drag = drag.clone();
                move |bounds, window, cx| {
                    drag.update(cx, |drag, _| drag.bounds = bounds);
                    window.insert_hitbox(bounds, HitboxBehavior::Normal)
                }
            },
            {
                let drag = drag.clone();
                let emit = Rc::clone(&emit);
                let release = Rc::clone(&release);
                move |_, hitbox, window, _| {
                    let moved = drag.clone();
                    let mouse_emit = Rc::clone(&emit);
                    let mouse_release = Rc::clone(&release);
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        let Drag {
                            bounds,
                            live,
                            touch,
                            ..
                        } = *moved.read(cx);
                        if !phase.bubble() || live.is_none() || touch {
                            return;
                        }
                        if event.pressed_button == Some(MouseButton::Left) {
                            mouse_emit(scale.at(event.position.x, bounds, thumb), window, cx);
                        } else {
                            mouse_release(window, cx);
                        }
                    });
                    let released = drag.clone();
                    let up_release = Rc::clone(&release);
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        let Drag { live, touch, .. } = *released.read(cx);
                        if phase.bubble() && live.is_some() && !touch {
                            up_release(window, cx);
                        }
                    });
                    window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        let Drag { bounds, touch, .. } = *drag.read(cx);
                        match event.phase {
                            TouchPhase::Started => {
                                if window.default_prevented()
                                    || !hitbox.is_hovered_at(event.start_position, window)
                                {
                                    return;
                                }
                                window.prevent_default();
                                drag.update(cx, |drag, _| drag.touch = true);
                                emit(scale.at(event.position.x, bounds, thumb), window, cx);
                            }
                            TouchPhase::Moved if touch => {
                                emit(scale.at(event.position.x, bounds, thumb), window, cx);
                            }
                            TouchPhase::Ended | TouchPhase::Cancelled if touch => {
                                release(window, cx);
                            }
                            TouchPhase::Moved | TouchPhase::Ended | TouchPhase::Cancelled => {}
                        }
                    });
                }
            },
        )
        .absolute()
        .inset_0();
        let key_drag = drag.clone();
        let key_emit = Rc::clone(&emit);
        let key_release = Rc::clone(&release);
        div()
            .author_id(self.id)
            .role(Role::Slider)
            .aria_label(self.label)
            .aria_value(spoken)
            .aria_disabled(disabled)
            .relative()
            .flex()
            .items_center()
            .w_full()
            .map(|this| {
                if fat {
                    this.h(px(STOP_KNOB))
                } else {
                    this.h(metrics.size.control_h())
                }
            })
            .px(px(thumb / 2.0))
            .child(rail)
            .when(!disabled, |this| {
                this.track_focus(&focus)
                    .click_cursor()
                    .on_key_down(move |event, window, cx| {
                        let steps = if event.keystroke.modifiers.shift {
                            10.0
                        } else {
                            1.0
                        };
                        let next = match event.keystroke.key.as_str() {
                            "left" | "down" => scale.nudge(value, -steps),
                            "right" | "up" => scale.nudge(value, steps),
                            "home" => scale.snap(scale.min),
                            "end" => scale.snap(scale.max),
                            _ => return,
                        };
                        cx.stop_propagation();
                        key_drag.update(cx, |drag, cx| drag.set_pointer(false, cx));
                        key_emit(next, window, cx);
                        key_release(window, cx);
                    })
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        focus.focus(window, cx);
                        drag.update(cx, |drag, cx| drag.set_pointer(true, cx));
                        let bounds = drag.read(cx).bounds;
                        emit(scale.at(event.position.x, bounds, thumb), window, cx);
                    })
                    .child(listeners)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{
        Context, Modifiers, MouseDownEvent, Render, ScaledPixels, TestAppContext,
        VisualTestContext, point,
    };
    use std::cell::RefCell;

    struct SliderTest {
        value: Rc<RefCell<f32>>,
        changes: Rc<RefCell<Vec<f32>>>,
        commit_on_release: bool,
    }

    impl Render for SliderTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let value = Rc::clone(&self.value);
            let changes = Rc::clone(&self.changes);
            let slider = Slider::new(
                "test-slider",
                "Hue",
                *self.value.borrow(),
                0.0..=360.0,
                move |next, _, _| {
                    *value.borrow_mut() = next;
                    changes.borrow_mut().push(next);
                },
            )
            .step(1.0);
            div().w(px(216.0)).child(if self.commit_on_release {
                slider.commit_on_release()
            } else {
                slider
            })
        }
    }

    #[ala::test]
    fn clicking_dragging_and_arrow_keys_move_the_value(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let value = Rc::new(RefCell::new(0.0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let (value, changes) = (Rc::clone(&value), Rc::clone(&changes));
            move |_, _| SliderTest {
                value,
                changes,
                commit_on_release: false,
            }
        });
        let cx: &mut VisualTestContext = cx;
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        };
        draw(cx);
        let y = px(12.0);
        cx.simulate_event(MouseDownEvent {
            position: point(px(108.0), y),
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        });
        draw(cx);
        assert_eq!(*value.borrow(), 180.0);
        cx.simulate_event(MouseMoveEvent {
            position: point(px(400.0), y),
            modifiers: Modifiers::default(),
            pressed_button: Some(MouseButton::Left),
        });
        draw(cx);
        assert_eq!(*value.borrow(), 360.0);
        cx.simulate_event(MouseUpEvent {
            position: point(px(400.0), y),
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        });
        draw(cx);
        cx.simulate_event(MouseMoveEvent {
            position: point(px(0.0), y),
            modifiers: Modifiers::default(),
            pressed_button: None,
        });
        draw(cx);
        assert_eq!(*value.borrow(), 360.0);
        cx.simulate_keystrokes("left");
        draw(cx);
        assert_eq!(*value.borrow(), 359.0);
        cx.simulate_keystrokes("shift-left");
        assert_eq!(*value.borrow(), 349.0);
        assert_eq!(*changes.borrow(), [180.0, 360.0, 359.0, 349.0]);
    }

    struct RailTest(Rc<RefCell<Vec<usize>>>);

    impl Render for RailTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let picks = Rc::clone(&self.0);
            div().w(px(230.0)).child(StepRail::new(
                "effort",
                "Effort",
                ["Default", "Low", "Medium", "High", "Xhigh", "Max"]
                    .map(SharedString::from)
                    .to_vec(),
                Some(3),
                move |index, _, _| picks.borrow_mut().push(index),
            ))
        }
    }

    #[ala::test]
    fn a_step_rail_reports_the_stop_it_is_let_go_on_and_the_end_key(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let picks = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let picks = Rc::clone(&picks);
            move |_, _| RailTest(picks)
        });
        let cx: &mut VisualTestContext = cx;
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        };
        draw(cx);
        let y = px(STOP_HEAD + STOP_KNOB / 2.0);
        cx.simulate_event(MouseDownEvent {
            position: point(px(175.0), y),
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        });
        draw(cx);
        cx.simulate_event(MouseMoveEvent {
            position: point(px(96.0), y),
            modifiers: Modifiers::default(),
            pressed_button: Some(MouseButton::Left),
        });
        draw(cx);
        assert!(picks.borrow().is_empty());
        cx.simulate_event(MouseUpEvent {
            position: point(px(96.0), y),
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        });
        draw(cx);
        cx.simulate_keystrokes("end");
        assert_eq!(*picks.borrow(), [2, 5]);
    }

    #[ala::test]
    fn a_touch_drag_that_starts_on_the_slider_moves_it(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let value = Rc::new(RefCell::new(0.0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let (value, changes) = (Rc::clone(&value), Rc::clone(&changes));
            move |_, _| SliderTest {
                value,
                changes,
                commit_on_release: false,
            }
        });
        let cx: &mut VisualTestContext = cx;
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        };
        let touch = |cx: &mut VisualTestContext, phase, start: f32, x: f32| {
            cx.simulate_event(TouchDragEvent {
                phase,
                start_position: point(px(108.0), px(start)),
                position: point(px(x), px(start)),
            });
            draw(cx);
        };
        draw(cx);
        touch(cx, TouchPhase::Started, 200.0, 108.0);
        touch(cx, TouchPhase::Moved, 200.0, 400.0);
        touch(cx, TouchPhase::Ended, 200.0, 400.0);
        assert!(changes.borrow().is_empty());
        touch(cx, TouchPhase::Started, 12.0, 108.0);
        touch(cx, TouchPhase::Moved, 12.0, 400.0);
        touch(cx, TouchPhase::Ended, 12.0, 400.0);
        touch(cx, TouchPhase::Moved, 12.0, 0.0);
        assert_eq!(*value.borrow(), 360.0);
        assert_eq!(*changes.borrow(), [180.0, 360.0]);
    }

    #[ala::test]
    fn a_slider_that_commits_on_release_reports_once_the_drag_lets_go(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let value = Rc::new(RefCell::new(0.0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let (value, changes) = (Rc::clone(&value), Rc::clone(&changes));
            move |_, _| SliderTest {
                value,
                changes,
                commit_on_release: true,
            }
        });
        let cx: &mut VisualTestContext = cx;
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        };
        let touch = |cx: &mut VisualTestContext, phase, x: f32| {
            cx.simulate_event(TouchDragEvent {
                phase,
                start_position: point(px(108.0), px(12.0)),
                position: point(px(x), px(12.0)),
            });
            draw(cx);
        };
        draw(cx);
        cx.simulate_event(MouseDownEvent {
            position: point(px(108.0), px(12.0)),
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        });
        draw(cx);
        assert!(changes.borrow().is_empty());
        cx.simulate_event(MouseUpEvent {
            position: point(px(108.0), px(12.0)),
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        });
        draw(cx);
        assert_eq!(*changes.borrow(), [180.0]);
        cx.simulate_keystrokes("left");
        draw(cx);
        assert_eq!(*changes.borrow(), [180.0, 179.0]);
        touch(cx, TouchPhase::Started, 108.0);
        touch(cx, TouchPhase::Moved, 160.0);
        touch(cx, TouchPhase::Moved, 400.0);
        assert_eq!(*changes.borrow(), [180.0, 179.0]);
        touch(cx, TouchPhase::Ended, 400.0);
        assert_eq!(*changes.borrow(), [180.0, 179.0, 360.0]);
    }

    #[ala::test]
    fn the_focus_ring_follows_the_keyboard_and_not_a_press(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| SliderTest {
            value: Rc::new(RefCell::new(0.0)),
            changes: Rc::new(RefCell::new(Vec::new())),
            commit_on_release: false,
        });
        let cx: &mut VisualTestContext = cx;
        let rings = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
                window
                    .painted_quads()
                    .iter()
                    .filter(|quad| quad.border_widths.top.0 > 0.0)
                    .map(|quad| quad.bounds)
                    .collect::<Vec<_>>()
            })
        };
        let click = |cx: &mut VisualTestContext| {
            cx.simulate_event(MouseDownEvent {
                position: point(px(108.0), px(12.0)),
                modifiers: Modifiers::default(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            });
            cx.simulate_event(MouseUpEvent {
                position: point(px(108.0), px(12.0)),
                modifiers: Modifiers::default(),
                button: MouseButton::Left,
                click_count: 1,
            });
        };
        rings(cx);
        click(cx);
        assert_eq!(rings(cx), []);
        cx.simulate_keystrokes("end");
        assert_eq!(
            rings(cx),
            [Bounds::new(
                point(ScaledPixels(397.0), ScaledPixels(9.0)),
                ala::size(ScaledPixels(38.0), ScaledPixels(38.0)),
            )]
        );
        click(cx);
        assert_eq!(rings(cx), []);
        cx.update(Window::blur);
        rings(cx);
        cx.update(Window::focus_next);
        assert_eq!(rings(cx).len(), 1);
    }

    struct DisabledSliders(Rc<RefCell<Vec<f32>>>);

    impl Render for DisabledSliders {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let (moved, stepped) = (Rc::clone(&self.0), Rc::clone(&self.0));
            div()
                .w(px(216.0))
                .child(
                    Slider::new("hue", "Hue", 0.0, 0.0..=360.0, move |next, _, _| {
                        moved.borrow_mut().push(next);
                    })
                    .disabled(true),
                )
                .child(
                    DiscreteSlider::new(
                        "steps",
                        "Size",
                        vec!["S".into(), "M".into(), "L".into()],
                        0,
                        move |index, _, _| stepped.borrow_mut().push(index as f32),
                    )
                    .disabled(true),
                )
        }
    }

    #[ala::test]
    fn disabled_sliders_ignore_presses_and_keys_and_take_no_focus(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let changes = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let changes = Rc::clone(&changes);
            move |_, _| DisabledSliders(changes)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let pill = cx
            .debug_bounds("steps:pill-2")
            .expect("the last pill is drawn");
        cx.simulate_click(point(px(108.0), px(14.0)), Modifiers::default());
        cx.simulate_click(pill.center(), Modifiers::default());
        cx.update(Window::focus_next);
        cx.simulate_keystrokes("right end");
        assert!(cx.update(|window, cx| window.focused(cx).is_none()));
        assert_eq!(*changes.borrow(), [0.0; 0]);
    }

    #[test]
    fn values_snap_to_their_step_inside_the_range() {
        let scale = Scale {
            min: 0.0,
            max: 360.0,
            step: 1.0,
        };
        assert_eq!(scale.snap(145.4), 145.0);
        assert_eq!(scale.snap(-3.0), 0.0);
        assert_eq!(scale.snap(400.0), 360.0);
        assert_eq!(scale.nudge(359.0, 10.0), 360.0);
        let bounds = Bounds::new(
            ala::point(px(10.0), px(0.0)),
            ala::size(px(216.0), px(24.0)),
        );
        assert_eq!(
            scale.at(px(18.0), bounds, Metrics::new(Size::Medium, false).knob),
            0.0
        );
        assert_eq!(
            scale.at(px(118.0), bounds, Metrics::new(Size::Medium, false).knob),
            180.0
        );
        assert_eq!(
            scale.at(px(400.0), bounds, Metrics::new(Size::Medium, false).knob),
            360.0
        );
    }
}
