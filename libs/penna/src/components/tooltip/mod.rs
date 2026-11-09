use std::rc::Rc;

use crate::Space;
use ala::{
    Action, AnyTooltip, AnyView, App, AppContext as _, AsKeystroke as _, Context, ElementId,
    InteractiveElement, IntoElement, KeyContext, Keystroke, ParentElement, Render, SharedString,
    StyleRefinement, Styled, Window, canvas, div, point, prelude::FluentBuilder,
};

use crate::{
    ActiveTheme as _, Sizable as _, StyledExt as _, Surface, SurfaceRole, TextStyle,
    components::kbd::Kbd, h_flex,
};

pub struct Tooltip {
    style: StyleRefinement,
    text: SharedString,
    action: Option<(Box<dyn Action>, Option<SharedString>)>,
    keys: Vec<Keystroke>,
}

impl Tooltip {
    #[must_use]
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            style: StyleRefinement::default(),
            text: text.into(),
            action: None,
            keys: Vec::new(),
        }
    }

    #[must_use]
    pub fn keys(mut self, keys: impl IntoIterator<Item = Keystroke>) -> Self {
        self.keys = keys.into_iter().collect();
        self
    }

    #[must_use]
    pub fn action(mut self, action: &dyn Action, context: Option<&str>) -> Self {
        self.action = Some((action.boxed_clone(), context.map(SharedString::new)));
        self
    }

    #[must_use]
    pub fn build(self, _window: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl FluentBuilder for Tooltip {}

impl Styled for Tooltip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bound = self
            .action
            .as_ref()
            .map(|(action, context)| bound_keys(action.as_ref(), context.as_deref(), window))
            .unwrap_or_default();
        let keys = if self.keys.is_empty() {
            bound
        } else {
            self.keys.clone()
        };
        let surface = Surface::new(SurfaceRole::TOOLTIP);
        let ground = cx.theme().ground_of(surface);

        div().child(
            h_flex()
                .m(Space::S12)
                .px(Space::S10)
                .py(Space::S4)
                .gap(Space::S12)
                .justify_between()
                .font_family(cx.theme().font_family.clone())
                .type_style(TextStyle::Small, cx)
                .surface(surface, cx)
                .refine_style(&self.style)
                .child(div().child(self.text.clone()))
                .when(!keys.is_empty(), |this| {
                    this.child(
                        h_flex()
                            .flex_shrink_0()
                            .gap(Space::GAP_TIGHT)
                            .children(keys.into_iter().map(|key| Kbd::new(key).small().on(ground))),
                    )
                }),
        )
    }
}

fn bound_keys(action: &dyn Action, context: Option<&str>, window: &Window) -> Vec<Keystroke> {
    match context.and_then(|context| KeyContext::parse(context).ok()) {
        Some(context) => window.highest_precedence_binding_for_action_in_context(action, context),
        None => window.highest_precedence_binding_for_action(action),
    }
    .map(|binding| {
        binding
            .keystrokes()
            .iter()
            .map(|key| key.as_keystroke().clone())
            .collect()
    })
    .unwrap_or_default()
}

#[derive(Default)]
struct FocusTip {
    dismissed: bool,
}

pub(crate) fn with_focus_tooltip<E: ParentElement + InteractiveElement + Styled>(
    mut this: E,
    id: &ElementId,
    focused: bool,
    text: Option<SharedString>,
    keys: &[Keystroke],
    window: &mut Window,
    cx: &mut App,
) -> E {
    let Some(text) = text else {
        return this;
    };
    let state = window.use_keyed_state(id.clone(), cx, |_, _| FocusTip::default());
    let keyboard = focused && window.last_input_was_keyboard();
    if !keyboard && state.read(cx).dismissed != focused {
        state.update(cx, |tip, _| tip.dismissed = focused);
    }
    if !focused {
        return this;
    }
    let shown = keyboard && !state.read(cx).dismissed;
    let border = Styled::style(&mut this)
        .border_widths
        .bottom
        .map(|width| width.to_pixels(window.rem_size()))
        .unwrap_or_default();
    let this = this.on_key_down(move |event, _, cx| {
        if event.keystroke.key == "escape" {
            state.update(cx, |tip, cx| {
                if !tip.dismissed {
                    tip.dismissed = true;
                    cx.notify();
                }
            });
        }
    });
    if !shown {
        return this;
    }
    let view = Tooltip::new(text).keys(keys.to_vec()).build(window, cx);
    this.child(
        canvas(
            move |bounds, window, _| {
                window.set_tooltip(AnyTooltip {
                    view,
                    mouse_position: point(bounds.center().x, bounds.bottom() + border),
                    check_visible_and_update: Rc::new(|_, _, _| true),
                });
            },
            |_, (), _, _| {},
        )
        .absolute()
        .inset_0(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        IconName,
        components::button::{Button, ButtonVariants as _},
    };
    use ala::{
        Modifiers, MouseMoveEvent, Point, ScaledPixels, TestAppContext, VisualTestContext, px,
    };

    struct Host;

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().p(px(20.0)).child(
                Button::new("split")
                    .icon(IconName::PanelRight)
                    .ghost()
                    .tooltip("Split right"),
            )
        }
    }

    #[ala::test]
    fn keyboard_focus_shows_the_tooltip_until_the_pointer_or_escape(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Host);
        let cx: &mut VisualTestContext = cx;
        let tip = point(ScaledPixels(94.0), ScaledPixels(122.0));
        let shown = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
                window
                    .painted_quads()
                    .iter()
                    .any(|quad| quad.bounds.origin == tip)
            })
        };
        let tab = |cx: &mut VisualTestContext| {
            cx.simulate_keystrokes("tab");
            cx.update(Window::focus_next);
        };
        assert!(!shown(cx));
        tab(cx);
        assert!(shown(cx));
        cx.simulate_event(MouseMoveEvent {
            position: Point::default(),
            modifiers: Modifiers::default(),
            pressed_button: None,
        });
        assert!(!shown(cx));
        cx.simulate_keystrokes("x");
        assert!(!shown(cx));
        cx.update(Window::blur);
        assert!(!shown(cx));
        tab(cx);
        assert!(shown(cx));
        cx.simulate_keystrokes("escape");
        assert!(!shown(cx));
    }
}
