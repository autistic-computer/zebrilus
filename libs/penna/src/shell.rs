use crate::{ActiveTheme as _, InteractiveElementExt};
use ala::{
    AnyElement, App, ElementId, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    MouseMoveEvent, ParentElement as _, Stateful, Styled as _, Window, WindowControlArea, div,
    prelude::FluentBuilder,
};

const BLURRED_CHROME_OPACITY: f32 = 0.93;

#[allow(
    clippy::disallowed_methods,
    reason = "blurred chrome lets the window material show through"
)]
pub fn chrome_background(background: Hsla, blur_opacity: Option<f32>) -> Hsla {
    match blur_opacity {
        Some(opacity) => background.opacity(
            BLURRED_CHROME_OPACITY + (1.0 - BLURRED_CHROME_OPACITY) * opacity.clamp(0.0, 1.0),
        ),
        None => background,
    }
}

pub fn app_shell_surface(
    id: impl Into<ElementId>,
    background: Hsla,
    sidebar: impl IntoElement,
    titlebar: Option<AnyElement>,
    content: impl IntoElement,
    overlays: impl IntoIterator<Item = AnyElement>,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .bg(background)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(sidebar)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .children(titlebar)
                        .child(div().flex().flex_1().min_w_0().min_h_0().child(content)),
                ),
        )
        .children(overlays)
}

pub fn app_titlebar_strip(
    id: impl Into<ElementId>,
    leading: impl IntoElement,
    controls: impl IntoElement,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .w_full()
        .h(cx.theme().title_bar_height())
        .items_center()
        .child(
            div()
                .flex()
                .flex_1()
                .h_full()
                .min_w_0()
                .items_center()
                .window_control_area(WindowControlArea::Drag)
                .child(leading),
        )
        .child(controls)
}

struct DragCandidate {
    armed: bool,
    accepts_click: bool,
}

pub fn window_drag_handle<E: InteractiveElementExt + FluentBuilder>(
    key: impl Into<ElementId>,
    strip: E,
    window: &mut Window,
    cx: &mut App,
) -> E {
    if cfg!(target_os = "windows") {
        return strip;
    }
    let state = window.use_keyed_state(key.into(), cx, |_, _| DragCandidate {
        armed: false,
        accepts_click: false,
    });
    strip
        .when(cfg!(target_os = "linux"), |strip| {
            strip.on_double_click(window.listener_for(&state, |state, _, window, _| {
                if state.accepts_click {
                    window.zoom_window();
                }
            }))
        })
        .when(cfg!(target_os = "macos"), |strip| {
            strip.on_double_click(window.listener_for(&state, |state, _, window, _| {
                if state.accepts_click {
                    window.titlebar_double_click();
                }
            }))
        })
        .on_mouse_down_out(window.listener_for(&state, |state, _, _, _| {
            state.armed = false;
            state.accepts_click = false;
        }))
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(&state, |state, _, window, _| {
                state.accepts_click = !window.default_prevented();
                state.armed = state.accepts_click;
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(&state, |state, _, _, _| {
                state.armed = false;
            }),
        )
        .on_mouse_move(
            window.listener_for(&state, |state, event: &MouseMoveEvent, window, _| {
                if state.armed {
                    state.armed = false;
                    if event.pressed_button == Some(MouseButton::Left) {
                        window.start_window_move();
                    }
                }
            }),
        )
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;
    use crate::{
        button::Button,
        h_flex,
        input::{Input, InputState},
    };
    use ala::{AppContext as _, Context, Entity, FocusHandle, Render, px};

    struct Preview {
        focus: FocusHandle,
        input: Entity<InputState>,
        drag: Option<Entity<DragCandidate>>,
    }

    impl Render for Preview {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let drag = window_drag_handle(
                "drag-test",
                div()
                    .id("drag-test-strip")
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(crate::TITLE_BAR_HEIGHT),
                window,
                cx,
            );
            self.drag = Some(window.use_keyed_state("drag-test", cx, |_, _| unreachable!()));
            div()
                .track_focus(&self.focus)
                .relative()
                .w(px(400.0))
                .h(px(200.0))
                .child(drag)
                .child(
                    h_flex()
                        .h(crate::TITLE_BAR_HEIGHT)
                        .child(
                            div()
                                .w(px(80.0))
                                .debug_selector(|| "drag-button".into())
                                .child(Button::new("drag-button").label("Reset")),
                        )
                        .child(
                            div()
                                .w(px(120.0))
                                .debug_selector(|| "drag-input".into())
                                .child(Input::new(&self.input)),
                        )
                        .child(
                            div()
                                .id("drag-own")
                                .w(px(100.0))
                                .h_full()
                                .debug_selector(|| "drag-own".into())
                                .on_mouse_down(MouseButton::Left, |_, window, _| {
                                    window.prevent_default();
                                }),
                        ),
                )
        }
    }

    #[ala::test]
    fn background_drag_yields_to_buttons_text_fields_and_controls_that_claim_the_press(
        cx: &mut ala::TestAppContext,
    ) {
        cx.update(crate::init);
        let (preview, cx) = cx.add_window_view(|window, cx| Preview {
            focus: cx.focus_handle(),
            input: cx.new(|cx| InputState::new(window, cx)),
            drag: None,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let state = preview.read_with(cx, |preview, _| preview.drag.clone().unwrap());
        let background = ala::point(px(360.0), px(15.0));
        cx.simulate_mouse_down(background, MouseButton::Left, ala::Modifiers::default());
        assert!(state.read_with(cx, |state: &DragCandidate, _| state.armed));
        cx.simulate_mouse_up(background, MouseButton::Left, ala::Modifiers::default());
        assert!(!state.read_with(cx, |state, _| state.armed));

        for selector in ["drag-button", "drag-input", "drag-own"] {
            let bounds = cx.debug_bounds(selector).unwrap();
            cx.simulate_mouse_down(
                bounds.center(),
                MouseButton::Left,
                ala::Modifiers::default(),
            );
            assert!(!state.read_with(cx, |state, _| state.armed), "{selector}");
            assert!(
                !state.read_with(cx, |state, _| state.accepts_click),
                "{selector}"
            );
            cx.simulate_mouse_move(
                bounds.center(),
                Some(MouseButton::Left),
                ala::Modifiers::default(),
            );
            cx.simulate_mouse_up(
                bounds.center(),
                MouseButton::Left,
                ala::Modifiers::default(),
            );
        }
    }
}
