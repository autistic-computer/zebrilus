use std::{rc::Rc, time::Duration};

use ala::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ClickEvent, ElementId,
    FULL_CORNER_RADIUS, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Disableable, Emphasis, Ground, Kind, Layer, PaintState, Sizable, Size, Space,
    StyledExt as _, Surface, SurfaceRole, Tone, components::input::field_layer,
    foundation::focus::with_focus_ring, h_flex, rems_from_px,
};

const SLIDE: Duration = Duration::from_millis(220);
const DIM: f32 = 0.5;

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    segments: Vec<SharedString>,
    selected: usize,
    on: Option<Ground>,
    size: Size,
    disabled: bool,
    full_width: bool,
    on_select: Option<SelectHandler>,
}

#[derive(Default)]
struct Slide {
    segments: Vec<Bounds<Pixels>>,
    shown: Option<usize>,
    from: Option<Bounds<Pixels>>,
    generation: u64,
}

impl SegmentedControl {
    #[must_use]
    pub fn new(
        id: impl Into<ElementId>,
        segments: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Self {
            id: id.into(),
            segments: segments.into_iter().map(Into::into).collect(),
            selected: 0,
            on: None,
            size: Size::Medium,
            disabled: false,
            full_width: false,
            on_select: None,
        }
    }

    #[must_use]
    pub const fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    #[must_use]
    pub const fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }

    #[must_use]
    pub const fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    #[must_use]
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl Sizable for SegmentedControl {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for SegmentedControl {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn lerp(from: Pixels, to: Pixels, delta: f32) -> Pixels {
    from + (to - from) * delta
}

fn dimmed(mut layer: Layer) -> Layer {
    layer.fill = layer.fill.opacity(DIM);
    layer.border = layer.border.opacity(DIM);
    layer.text = layer.text.opacity(DIM);
    layer.shadow.clear();
    layer
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let track = Surface::new(Kind::Field.role()).on_maybe(self.on);
        let layer = cx.theme().layer(track, PaintState::Rest);
        let ground = layer.ground;
        let border = layer.border_width.max(px(1.0));
        let size = self.size;
        let disabled = self.disabled;
        let full_width = self.full_width;
        let height = rems_from_px(size.control_px()).to_pixels(window.rem_size());
        let inset = Space::S2.rems().to_pixels(window.rem_size());
        let inner = height - inset * 2.0 - border * 2.0;
        let radius = if layer.radius >= FULL_CORNER_RADIUS {
            FULL_CORNER_RADIUS
        } else {
            crate::foundation::nested_radius(
                crate::foundation::row_radius(&layer, height),
                border + inset,
                cx.theme().control_corner(inner),
            )
        };
        let pill = Surface::new(SurfaceRole::QUIET_BUTTON).on(ground);
        let picked = cx.theme().layer(pill, PaintState::Current);
        let (picked, rest) = if disabled {
            (
                dimmed(picked),
                cx.theme().layer(pill, PaintState::Disabled).text,
            )
        } else {
            (picked, cx.theme().layer(pill, PaintState::Rest).text)
        };
        let hover = cx.theme().layer(pill, PaintState::Hover);
        let (hover_fill, hover) = (hover.fill, hover.text);
        let pressed_fill = cx.theme().layer(pill, PaintState::Active).fill;
        let smoothing = picked.smoothing;
        let coarse = crate::blocks::touch::CoarsePointer::get(cx);
        let slop = crate::blocks::touch::control_slop(size);
        let ring = cx.theme().paint_on(
            self.on.unwrap_or_else(|| cx.theme().ground()),
            Tone::Neutral,
            Emphasis::Ghost,
        );
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let on_select = self.on_select.filter(|_| !disabled);
        let focused = on_select.is_some() && focus.is_focused(window);

        let last = self.segments.len().saturating_sub(1);
        let selected = self.selected.min(last);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| Slide::default());
        let (to, from, generation) = state.update(cx, |slide, _| {
            let to = slide.segments.get(selected).copied();
            if to.is_some() && slide.shown != Some(selected) {
                slide.from = slide
                    .shown
                    .and_then(|shown| slide.segments.get(shown).copied());
                slide.shown = Some(selected);
                slide.generation += 1;
            }
            (to, slide.from, slide.generation)
        });
        let pill = to.map(|to| {
            let pill = div()
                .absolute()
                .top_0()
                .h(inner)
                .layer(&picked)
                .map(|pill| crate::foundation::shared_corners(pill, radius, inner, smoothing));
            match from {
                Some(from) if from != to => pill
                    .with_animation(
                        ("slide", generation),
                        Animation::new(SLIDE).with_easing(ala::ease_out_quint()),
                        move |pill, delta| {
                            pill.left(lerp(from.origin.x, to.origin.x, delta)).w(lerp(
                                from.size.width,
                                to.size.width,
                                delta,
                            ))
                        },
                    )
                    .into_any_element(),
                _ => pill.left(to.origin.x).w(to.size.width).into_any_element(),
            }
        });

        let skip = usize::from(pill.is_some());
        let measured = state.clone();
        let segments = self
            .segments
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let picked_text = picked.text;
                let click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>> =
                    on_select.clone().map(|on_select| {
                        Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            on_select(index, window, cx);
                        }) as Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>
                    });
                div()
                    .id(("segment", index))
                    .role(ala::Role::Button)
                    .aria_label(label.clone())
                    .aria_selected(index == selected)
                    .relative()
                    .flex()
                    .map(|segment| {
                        if full_width {
                            segment.flex_1().flex_basis(px(0.))
                        } else {
                            segment.flex_none()
                        }
                    })
                    .items_center()
                    .justify_center()
                    .h(inner)
                    .px(rems_from_px(
                        size.field_inset().design_px() + Space::S2.design_px(),
                    ))
                    .type_style(size.control_text(), cx)
                    .whitespace_nowrap()
                    .map(|segment| {
                        if index == selected {
                            segment.text_color(picked_text)
                        } else if disabled {
                            segment.text_color(rest)
                        } else {
                            crate::foundation::shared_corners(
                                segment.text_color(rest),
                                radius,
                                inner,
                                smoothing,
                            )
                            .hover(move |style| style.bg(hover_fill).text_color(hover))
                            .active(move |style| style.bg(pressed_fill))
                        }
                    })
                    .child(label)
                    .when_some(click, |segment, click| {
                        let tap = Rc::clone(&click);
                        segment
                            .when(!disabled, crate::StyledExt::click_cursor)
                            .on_click(move |event, window, cx| click(event, window, cx))
                            .when(coarse && slop > 0.0, |segment| {
                                segment.child(
                                    crate::blocks::touch::hit_area("touch", 0.0, slop)
                                        .on_click(move |event, window, cx| tap(event, window, cx)),
                                )
                            })
                    })
                    .into_any_element()
            })
            .collect::<Vec<AnyElement>>();

        let chrome = field_layer(
            cx.theme(),
            track,
            if disabled {
                PaintState::Disabled
            } else if focused {
                PaintState::Focused
            } else {
                PaintState::Rest
            },
            true,
        );
        let mut hover = field_layer(cx.theme(), track, PaintState::Hover, true);
        hover.border = chrome.border;
        let mut held = field_layer(cx.theme(), track, PaintState::Active, true);
        held.border = chrome.border;
        let control = h_flex()
            .id(self.id)
            .map(|control| {
                if full_width {
                    control.w_full()
                } else {
                    control.flex_none()
                }
            })
            .p(Space::S2)
            .layer(&chrome)
            .when(!disabled, |this| {
                this.hover(move |style| crate::foundation::restyle(style, &hover))
                    .active(move |style| crate::foundation::restyle(style, &held))
            })
            .role(Role::RadioGroup)
            .aria_disabled(disabled)
            .when_some(on_select, |this, on_select| {
                this.track_focus(&focus.tab_stop(true))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_key_down(move |event, window, cx| {
                        if event.keystroke.modifiers.modified() {
                            return;
                        }
                        let next = match event.keystroke.key.as_str() {
                            "left" | "up" => selected.saturating_sub(1),
                            "right" | "down" => (selected + 1).min(last),
                            "home" => 0,
                            "end" => last,
                            _ => return,
                        };
                        cx.stop_propagation();
                        if next != selected {
                            on_select(next, window, cx);
                        }
                    })
            })
            .child(
                h_flex()
                    .relative()
                    .when(full_width, ala::Styled::flex_1)
                    .children(pill)
                    .children(segments)
                    .on_children_prepainted(move |bounds, window, cx| {
                        let Some(first) = bounds.get(skip).copied() else {
                            return;
                        };
                        let segments = bounds[skip..]
                            .iter()
                            .map(|bounds| Bounds::new(bounds.origin - first.origin, bounds.size))
                            .collect::<Vec<_>>();
                        let changed = measured.update(cx, |slide, _| {
                            let changed = slide.segments != segments;
                            slide.segments = segments;
                            changed
                        });
                        if changed {
                            window.request_animation_frame();
                        }
                    }),
            );
        with_focus_ring(control, focused, px(0.), &ring, layer.smoothing, window)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ala::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point};

    use super::*;

    struct Modes(Rc<RefCell<Vec<usize>>>, bool);

    impl Render for Modes {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let picks = Rc::clone(&self.0);
            let selected = picks.borrow().last().copied().unwrap_or(1);
            div().size_full().child(
                SegmentedControl::new("mode", ["System", "Light", "Dark"])
                    .selected(selected)
                    .disabled(self.1)
                    .on_select(move |index, _, _| picks.borrow_mut().push(index)),
            )
        }
    }

    #[ala::test]
    fn arrows_home_and_end_move_the_pick_and_a_click_picks_without_focus(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let picks = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let picks = Rc::clone(&picks);
            move |_, _| Modes(picks, false)
        });
        let cx: &mut VisualTestContext = cx;
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        draw(cx);
        cx.simulate_click(point(px(12.), px(14.)), Modifiers::default());
        draw(cx);
        assert_eq!(*picks.borrow(), [0]);
        assert!(cx.update(|window, cx| window.focused(cx).is_none()));
        cx.update(Window::focus_next);
        draw(cx);
        for key in [
            "right",
            "end",
            "right",
            "home",
            "left",
            "down",
            "shift-right",
        ] {
            cx.simulate_keystrokes(key);
            draw(cx);
        }
        assert_eq!(*picks.borrow(), [0, 1, 2, 0, 1]);
    }

    #[ala::test]
    fn a_disabled_control_ignores_clicks_and_keys_and_takes_no_focus(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let picks = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let picks = Rc::clone(&picks);
            move |_, _| Modes(picks, true)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_click(point(px(12.), px(14.)), Modifiers::default());
        cx.update(Window::focus_next);
        cx.simulate_keystrokes("right end home");
        assert!(cx.update(|window, cx| window.focused(cx).is_none()));
        assert_eq!(*picks.borrow(), [0; 0]);
    }
}
