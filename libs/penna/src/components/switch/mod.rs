use crate::Space;
use std::{rc::Rc, time::Duration};

use ala::{
    Animation, AnimationExt as _, App, Background, ClickEvent, ElementId, Entity, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Pixels,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, accesskit::Toggled, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Disableable, Emphasis, PaintState, Sizable, Size,
    Smoothing, StyledExt as _, Tone, foundation::focus::with_focus_ring, h_flex, oklab_lightness,
};

const TOGGLE_ANIMATION: Duration = Duration::from_millis(150);

type ClickHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

struct Metrics {
    track_w: Pixels,
    track_h: Pixels,
    thumb: Pixels,
    inset: Pixels,
}

impl Metrics {
    fn new(size: Size, coarse: bool) -> Self {
        let (track_w, track_h) = match size.rung() {
            Size::XSmall => (24., 14.),
            Size::Small => (28., 16.),
            _ if coarse => (44., 26.),
            Size::Large => (40., 22.),
            Size::XLarge | Size::Touch => (44., 26.),
            _ => (36., 20.),
        };
        let inset = px(2.);
        Self {
            track_w: px(track_w),
            track_h: px(track_h),
            thumb: px(track_h) - inset * 2,
            inset,
        }
    }

    fn travel(&self) -> Pixels {
        self.track_w - self.thumb - self.inset * 2
    }
}

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    style: StyleRefinement,
    checked: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
    size: Size,
    tooltip: Option<SharedString>,
    label: Option<SharedString>,
}

impl Switch {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            checked: false,
            disabled: false,
            on_click: None,
            size: Size::Medium,
            tooltip: None,
            label: None,
        }
    }

    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    #[must_use]
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&bool, &mut Window, &mut App) + 'static,
    {
        self.on_click = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

impl Styled for Switch {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Switch {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Switch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

fn thumb(
    checked: bool,
    disabled: bool,
    fill: Background,
    m: &Metrics,
    toggle_state: &Entity<bool>,
    cx: &mut App,
) -> impl IntoElement {
    let travel = m.travel();
    div()
        .rounded_full()
        .bg(fill)
        .shadow(crate::lifted_shadow(cx))
        .size(m.thumb)
        .map(|this| {
            if disabled || *toggle_state.read(cx) == checked {
                let x = if checked { travel } else { px(0.) };
                return this.left(x).into_any_element();
            }

            let duration = TOGGLE_ANIMATION;
            cx.spawn({
                let toggle_state = toggle_state.clone();
                async move |cx| {
                    cx.background_executor().timer(duration).await;
                    let () = toggle_state.update(cx, |this, _| *this = checked);
                }
            })
            .detach();

            this.with_animation(
                ElementId::NamedInteger("move".into(), u64::from(checked)),
                Animation::new(duration),
                move |this, delta| {
                    let x = if checked {
                        travel * delta
                    } else {
                        travel - travel * delta
                    };
                    this.left(x)
                },
            )
            .into_any_element()
        })
}

fn lighter(a: Hsla, b: Hsla) -> Hsla {
    if oklab_lightness(a) >= oklab_lightness(b) {
        a
    } else {
        b
    }
}

fn hold(_: &MouseDownEvent, window: &mut Window, cx: &mut App) {
    cx.stop_propagation();
    window.prevent_default();
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let coarse = crate::blocks::touch::CoarsePointer::get(cx);
        let m = Metrics::new(self.size, coarse);

        let toggle_state = window.use_keyed_state(self.id.clone(), cx, |_, _| checked);
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let on_click = self.on_click.filter(|_| !self.disabled);
        let focused = on_click.is_some() && focus_handle.is_focused(window);

        let theme = cx.theme();
        let paint = if checked {
            theme.paint(Tone::Primary, Emphasis::Solid)
        } else {
            theme.paint(Tone::Neutral, Emphasis::Soft)
        };
        let state = if self.disabled {
            PaintState::Disabled
        } else {
            PaintState::Rest
        };
        let track: Background = if checked {
            paint.fill_at(state)
        } else {
            paint
                .fill_at(PaintState::Active)
                .opacity(if self.disabled { 0.5 } else { 1.0 })
        }
        .into();
        let knob = if checked {
            paint.text_at(state)
        } else {
            lighter(theme.background, theme.foreground)
        };
        let thumb_fill: Background = if self.disabled {
            knob.opacity(0.5)
        } else {
            knob
        }
        .into();
        let ring = theme.paint(Tone::Neutral, Emphasis::Ghost);

        let track = div()
            .id(self.id.clone())
            .w(m.track_w)
            .h(m.track_h)
            .rounded_full()
            .flex()
            .items_center()
            .border(m.inset)
            .border_color(cx.theme().transparent)
            .bg(track)
            .when_some(self.tooltip.clone(), |this, tooltip| {
                this.tooltip(move |window, cx| {
                    crate::components::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
            .child(thumb(
                checked,
                self.disabled,
                thumb_fill,
                &m,
                &toggle_state,
                cx,
            ));

        div().refine_style(&self.style).child(
            h_flex()
                .author_id(self.id.clone())
                .role(Role::Switch)
                .aria_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .aria_disabled(self.disabled)
                .when_some(self.label.or_else(|| self.tooltip.clone()), |this, name| {
                    this.aria_label(name)
                })
                .gap(Space::S8)
                .items_start()
                .child(with_focus_ring(
                    track,
                    focused,
                    px(0.),
                    &ring,
                    Smoothing::CIRCLE.exponent(),
                    window,
                ))
                .when_some(on_click, |this, on_click| {
                    let toggle =
                        Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                            let () = toggle_state.update(cx, |this, _| *this = checked);
                            on_click(&!checked, window, cx);
                        });
                    let tap = Rc::clone(&toggle);
                    this.track_focus(&focus_handle.tab_stop(true))
                        .on_mouse_down(MouseButton::Left, hold)
                        .on_click(move |event, window, cx| toggle(event, window, cx))
                        .when(coarse, |this| {
                            this.child(
                                crate::blocks::touch::hit_area(
                                    "touch",
                                    (crate::blocks::touch::TOUCH_TARGET - f32::from(m.track_w))
                                        / 2.0,
                                    (crate::blocks::touch::TOUCH_TARGET - f32::from(m.track_h))
                                        / 2.0,
                                )
                                .on_mouse_down(MouseButton::Left, hold)
                                .on_click(move |event, window, cx| tap(event, window, cx)),
                            )
                        })
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ala::{
        Context, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseUpEvent, Render,
        TestAppContext, VisualTestContext, point,
    };

    use super::*;

    struct Switches;

    impl Render for Switches {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(Switch::new("off").label("Sharing"))
                .child(Switch::new("on").checked(true).disabled(true))
        }
    }

    #[ala::test]
    fn a_switch_announces_its_role_state_name_and_id(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Switches);
        let nodes = cx.update(|window, cx| {
            window.set_a11y_forced(true);
            window.draw(cx).clear(cx);
            let tree = window.a11y_tree().expect("forced a11y builds a tree");
            let mut nodes = tree
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == Role::Switch)
                .map(|(_, node)| {
                    (
                        node.author_id().map(str::to_owned),
                        node.toggled(),
                        node.label().map(str::to_owned),
                        node.is_disabled(),
                    )
                })
                .collect::<Vec<_>>();
            nodes.sort_by(|a, b| a.0.cmp(&b.0));
            nodes
        });
        assert_eq!(
            nodes,
            [
                (
                    Some("off".to_owned()),
                    Some(Toggled::False),
                    Some("Sharing".to_owned()),
                    false
                ),
                (Some("on".to_owned()), Some(Toggled::True), None, true),
            ]
        );
    }

    struct Flip(Rc<RefCell<Vec<bool>>>);

    impl Render for Flip {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let flips = Rc::clone(&self.0);
            div()
                .size_full()
                .child(Switch::new("flip").on_click(move |on, _, _| flips.borrow_mut().push(*on)))
        }
    }

    #[ala::test]
    fn a_switch_flips_on_release_and_on_space_or_enter_but_a_click_never_focuses_it(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let flips = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let flips = Rc::clone(&flips);
            move |_, _| Flip(flips)
        });
        let cx: &mut VisualTestContext = cx;
        let draw = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        let key = |cx: &mut VisualTestContext, key: &str| {
            let keystroke = Keystroke::parse(key).expect("valid keystroke");
            cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(KeyUpEvent { keystroke });
            draw(cx);
        };
        draw(cx);
        let center = point(px(18.), px(10.));
        cx.simulate_event(MouseDownEvent {
            position: center,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        });
        assert_eq!(*flips.borrow(), [false; 0]);
        cx.simulate_event(MouseUpEvent {
            position: center,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        });
        draw(cx);
        assert_eq!(*flips.borrow(), [true]);
        assert!(cx.update(|window, cx| window.focused(cx).is_none()));
        cx.update(Window::focus_next);
        draw(cx);
        key(cx, "space");
        key(cx, "enter");
        key(cx, "a");
        assert_eq!(*flips.borrow(), [true, true, true]);
    }
}
