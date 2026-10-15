use std::{ops::RangeInclusive, rc::Rc};

use zz_gpui::{
    App, Bounds, ElementId, FocusHandle, Hsla, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, RenderOnce, Role, SharedString, Window, canvas, div, linear_color_stop,
    linear_gradient, prelude::*, px, relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, PaintState, Size, StyledExt as _, Tone, h_flex,
    oklab_lightness, tooltip::Tooltip,
};

type ChangeHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type ValueHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;

pub(crate) const THUMB: f32 = 16.0;
const TRACK: f32 = 6.0;

#[derive(IntoElement)]
pub struct DiscreteSlider {
    id: ElementId,
    labels: Vec<SharedString>,
    selected: usize,
    label: SharedString,
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
            on_change: Rc::new(on_change),
        }
    }
}

impl RenderOnce for DiscreteSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = self.labels.len();
        let selected = self.selected.min(count.saturating_sub(1));
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle());
        let focus = state.read(cx).clone();
        let focused = focus.is_focused(window);
        let foreground = cx.theme().foreground;
        let filled = cx.theme().paint(Tone::Primary, Emphasis::Solid);
        let track = cx.theme().paint(Tone::Neutral, Emphasis::Soft);
        let value = self.labels.get(selected).cloned().unwrap_or_default();
        let mut pills = h_flex()
            .flex_1()
            .min_w_0()
            .max_w(px(
                count as f32 * 24.0 + count.saturating_sub(1) as f32 * 4.0
            ))
            .gap_1();
        for (index, label) in self.labels.iter().enumerate() {
            let change = self.on_change.clone();
            let focus = focus.clone();
            let tooltip = label.clone();
            let selector = format!("{}:pill-{index}", self.id);
            let group = SharedString::from(selector.clone());
            let (fill, hover) = if index <= selected {
                (filled.fill, filled.hover)
            } else {
                (track.fill_at(PaintState::Active), track.hover)
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
                    .h(Size::Medium.control_h())
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                    .child(
                        div()
                            .w_full()
                            .h(px(6.0))
                            .rounded(cx.theme().control_radius())
                            .bg(fill)
                            .group_hover(group, move |this| this.bg(hover))
                            .when(focused && index == selected, |this| {
                                this.focus_ring(&filled)
                            }),
                    )
                    .on_click(move |_, window, cx| {
                        focus.focus(window, cx);
                        change(index, window, cx);
                    }),
            );
        }
        h_flex()
            .id(self.id)
            .role(Role::Slider)
            .aria_label(self.label.clone())
            .aria_value(value.clone())
            .track_focus(&focus)
            .w_full()
            .gap_2()
            .text_xs()
            .line_height(px(16.0))
            .on_key_down(move |event, window, cx| {
                let next = match event.keystroke.key.as_str() {
                    "left" | "down" => selected.saturating_sub(1),
                    "right" | "up" => (selected + 1).min(count.saturating_sub(1)),
                    "home" => 0,
                    "end" => count.saturating_sub(1),
                    _ => return,
                };
                cx.stop_propagation();
                if count > 0 && next != selected {
                    (self.on_change)(next, window, cx);
                }
            })
            .child(
                div()
                    .flex_none()
                    .text_color(foreground.muted())
                    .child(self.label),
            )
            .child(pills)
            .child(
                div()
                    .flex_none()
                    .w(px(52.0))
                    .text_color(foreground)
                    .child(value),
            )
    }
}

/// A continuous slider: drag the thumb, click the track, or step it with the
/// arrow keys (ten steps with shift). Reports each new value, snapped to its
/// step, to `on_change` while the thumb moves.
#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    label: SharedString,
    value: f32,
    scale: Scale,
    track: Vec<Hsla>,
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
            on_change: Rc::new(on_change),
        }
    }

    #[must_use]
    pub fn step(mut self, step: f32) -> Self {
        self.scale.step = step.max(0.0);
        self
    }

    /// Paints the track as a gradient through `colors` instead of filling it
    /// up to the thumb.
    #[must_use]
    pub fn track(mut self, colors: impl IntoIterator<Item = Hsla>) -> Self {
        self.track = colors.into_iter().collect();
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

    fn at(self, x: Pixels, bounds: Bounds<Pixels>) -> f32 {
        let radius = px(THUMB / 2.0);
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
}

struct Drag {
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    live: Option<f32>,
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let drag = window.use_keyed_state(self.id.clone(), cx, |_, cx| Drag {
            focus: cx.focus_handle(),
            bounds: Bounds::default(),
            live: None,
        });
        let scale = self.scale;
        let focus = drag.read(cx).focus.clone();
        let focused = focus.is_focused(window);
        let value = scale.snap(drag.read(cx).live.unwrap_or(self.value));
        let fraction = scale.fraction(value);
        let theme = cx.theme();
        let filled = theme.paint(Tone::Primary, Emphasis::Solid);
        let rest = theme
            .paint(Tone::Neutral, Emphasis::Soft)
            .fill_at(PaintState::Active);
        let knob = if oklab_lightness(theme.background) >= oklab_lightness(theme.foreground) {
            theme.background
        } else {
            theme.foreground
        };
        let on_change = self.on_change;
        let emit: ValueHandler = Rc::new({
            let drag = drag.clone();
            move |next, window, cx| {
                let previous = drag.update(cx, |drag, cx| {
                    cx.notify();
                    drag.live.replace(next)
                });
                if previous != Some(next) {
                    on_change(next, window, cx);
                }
            }
        });
        let track = if self.track.len() > 1 {
            let last = self.track.len() - 2;
            div()
                .flex()
                .h(px(TRACK))
                .w_full()
                .children(self.track.windows(2).enumerate().map(|(index, pair)| {
                    div()
                        .flex_1()
                        .h_full()
                        .when(index == 0, Styled::rounded_l_full)
                        .when(index == last, Styled::rounded_r_full)
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(pair[0], 0.0),
                            linear_color_stop(pair[1], 1.0),
                        ))
                }))
        } else {
            div().h(px(TRACK)).w_full().rounded_full().bg(rest).child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(filled.fill),
            )
        };
        let thumb = div()
            .absolute()
            .top(px((TRACK - THUMB) / 2.0))
            .left(relative(fraction))
            .ml(px(-THUMB / 2.0))
            .size(px(THUMB))
            .rounded_full()
            .bg(knob)
            .shadow(crate::lifted_shadow(cx))
            .when(focused, |thumb| thumb.focus_ring(&filled));
        let listeners = canvas(
            {
                let drag = drag.clone();
                move |bounds, _, cx| drag.update(cx, |drag, _| drag.bounds = bounds)
            },
            {
                let drag = drag.clone();
                let emit = Rc::clone(&emit);
                move |_, (), window, _| {
                    let moved = drag.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        let Drag { bounds, live, .. } = *moved.read(cx);
                        if !phase.bubble() || live.is_none() {
                            return;
                        }
                        if event.pressed_button == Some(MouseButton::Left) {
                            emit(scale.at(event.position.x, bounds), window, cx);
                        } else {
                            moved.update(cx, |drag, _| drag.live = None);
                        }
                    });
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase.bubble() && drag.read(cx).live.is_some() {
                            drag.update(cx, |drag, cx| {
                                drag.live = None;
                                cx.notify();
                            });
                        }
                    });
                }
            },
        )
        .absolute()
        .inset_0();
        let key_emit = Rc::clone(&emit);
        let key_drag = drag.clone();
        div()
            .id(self.id)
            .role(Role::Slider)
            .aria_label(self.label)
            .aria_value(SharedString::from(format!("{value}")))
            .track_focus(&focus)
            .relative()
            .flex()
            .items_center()
            .w_full()
            .h(Size::Medium.control_h())
            .px(px(THUMB / 2.0))
            .cursor_pointer()
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
                key_emit(next, window, cx);
                key_drag.update(cx, |drag, _| drag.live = None);
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                focus.focus(window, cx);
                let bounds = drag.read(cx).bounds;
                emit(scale.at(event.position.x, bounds), window, cx);
            })
            .child(div().relative().w_full().child(track).child(thumb))
            .child(listeners)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use zz_gpui::{
        Context, Modifiers, MouseDownEvent, Render, TestAppContext, VisualTestContext, point,
    };

    struct SliderTest {
        value: Rc<RefCell<f32>>,
        changes: Rc<RefCell<Vec<f32>>>,
    }

    impl Render for SliderTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let value = Rc::clone(&self.value);
            let changes = Rc::clone(&self.changes);
            div().w(px(216.0)).child(
                Slider::new(
                    "test-slider",
                    "Hue",
                    *self.value.borrow(),
                    0.0..=360.0,
                    move |next, _, _| {
                        *value.borrow_mut() = next;
                        changes.borrow_mut().push(next);
                    },
                )
                .step(1.0),
            )
        }
    }

    #[zz_gpui::test]
    fn clicking_dragging_and_arrow_keys_move_the_value(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let value = Rc::new(RefCell::new(0.0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let (value, changes) = (Rc::clone(&value), Rc::clone(&changes));
            move |_, _| SliderTest { value, changes }
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
            zz_gpui::point(px(10.0), px(0.0)),
            zz_gpui::size(px(216.0), px(24.0)),
        );
        assert_eq!(scale.at(px(18.0), bounds), 0.0);
        assert_eq!(scale.at(px(118.0), bounds), 180.0);
        assert_eq!(scale.at(px(400.0), bounds), 360.0);
    }
}
